use rustbac_client::{
    schedule::{self, CalendarEntry, TimeValue},
    AlarmSummaryItem, AtomicReadFileResult, AtomicWriteFileResult, ClientDataValue,
    EnrollmentSummaryItem, EventState, ReadRangeResult, TimeStamp,
};
use rustbac_core::services::acknowledge_alarm::AcknowledgeAlarmRequest;
use rustbac_core::types::{ObjectId, ObjectType, PropertyId};

use crate::bridge::traits::BridgeError;
use crate::store::history_store::HistoryStore;

use super::conversion::trend_log_items_to_samples;
use super::loop_time_sync::now_bacnet_utc;

impl super::BacnetBridge {
    // -----------------------------------------------------------------------
    // TrendLog reading
    // -----------------------------------------------------------------------

    /// Read entries from a TrendLog object on a remote device.
    /// Returns (timestamp_ms, value) pairs suitable for history backfill.
    pub async fn read_trend_log(
        &self,
        device_instance: u32,
        trend_log_instance: u32,
        start_index: i32,
        count: i16,
    ) -> Result<Vec<(i64, f64)>, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let object_id = ObjectId::new(ObjectType::TrendLog, trend_log_instance);
        let result: ReadRangeResult = with_client!(tc, |c| c
            .read_range_by_position(
                dev.address,
                object_id,
                PropertyId::LogBuffer,
                None,
                start_index,
                count,
            )
            .await
            .map_err(|e| BridgeError::Protocol(format!("ReadRange failed: {e}"))))?;

        Ok(trend_log_items_to_samples(&result.items))
    }

    /// Get the record count of a TrendLog object.
    pub async fn trend_log_record_count(
        &self,
        device_instance: u32,
        trend_log_instance: u32,
    ) -> Result<u32, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let object_id = ObjectId::new(ObjectType::TrendLog, trend_log_instance);
        let val = with_client!(tc, |c| c
            .read_property(dev.address, object_id, PropertyId::RecordCount)
            .await
            .map_err(|e| BridgeError::Protocol(format!("ReadProperty RecordCount: {e}"))))?;
        match val {
            ClientDataValue::Unsigned(n) => Ok(n),
            _ => Ok(0),
        }
    }

    /// Backfill a TrendLog into the HistoryStore. Reads all records and inserts them.
    pub async fn backfill_trend_log(
        &self,
        device_instance: u32,
        trend_log_instance: u32,
        device_key: &str,
        point_id: &str,
        history_store: &HistoryStore,
    ) -> Result<usize, BridgeError> {
        let record_count = self
            .trend_log_record_count(device_instance, trend_log_instance)
            .await?;
        if record_count == 0 {
            return Ok(0);
        }

        let batch_size: i16 = 100;
        let mut total = 0usize;
        let mut index: i32 = 1; // BACnet ReadRange is 1-based

        while (index as u32) <= record_count {
            let remaining = record_count.saturating_sub(index as u32);
            let count = batch_size.min(remaining as i16 + 1);
            let samples = self
                .read_trend_log(device_instance, trend_log_instance, index, count)
                .await?;

            if samples.is_empty() {
                break;
            }

            let point_key = format!("{device_key}:{point_id}");
            let batch: Vec<(String, i64, f64)> = samples
                .iter()
                .map(|(ts, v)| (point_key.clone(), *ts, *v))
                .collect();
            total += batch.len();
            history_store.backfill(batch).await;

            index += count as i32;
        }

        tracing::info!(
            records = total,
            device_key,
            point_id,
            "BACnet: TrendLog backfill complete"
        );
        Ok(total)
    }

    // -----------------------------------------------------------------------
    // Schedule interop
    // -----------------------------------------------------------------------

    /// Read the weekly schedule from a BACnet Schedule object.
    pub async fn read_schedule(
        &self,
        device_instance: u32,
        schedule_instance: u32,
    ) -> Result<Vec<Vec<TimeValue>>, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let object_id = ObjectId::new(ObjectType::Schedule, schedule_instance);
        let val = with_client!(tc, |c| c
            .read_property(dev.address, object_id, PropertyId::WeeklySchedule)
            .await
            .map_err(|e| BridgeError::Protocol(format!("Read WeeklySchedule: {e}"))))?;
        schedule::decode_weekly_schedule(&val)
            .ok_or_else(|| BridgeError::Protocol("Failed to decode WeeklySchedule".into()))
    }

    /// Write a weekly schedule to a BACnet Schedule object.
    pub async fn write_schedule(
        &self,
        device_instance: u32,
        schedule_instance: u32,
        week: &[Vec<TimeValue>],
    ) -> Result<(), BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let object_id = ObjectId::new(ObjectType::Schedule, schedule_instance);
        let encoded = schedule::encode_weekly_schedule(week);
        with_client!(tc, |c| c
            .write_many(
                dev.address,
                &[(object_id, PropertyId::WeeklySchedule, encoded, None)],
            )
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "Write WeeklySchedule: {e}"
            ))))?;
        Ok(())
    }

    /// Read the default value from a BACnet Schedule object.
    pub async fn read_schedule_default(
        &self,
        device_instance: u32,
        schedule_instance: u32,
    ) -> Result<ClientDataValue, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let object_id = ObjectId::new(ObjectType::Schedule, schedule_instance);
        with_client!(tc, |c| c
            .read_property(dev.address, object_id, PropertyId::ScheduleDefault)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "Read ScheduleDefault: {e}"
            ))))
    }

    /// Read the date list from a BACnet Calendar object.
    pub async fn read_calendar(
        &self,
        device_instance: u32,
        calendar_instance: u32,
    ) -> Result<Vec<CalendarEntry>, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let object_id = ObjectId::new(ObjectType::Calendar, calendar_instance);
        let val = with_client!(tc, |c| c
            .read_property(dev.address, object_id, PropertyId::DateList)
            .await
            .map_err(|e| BridgeError::Protocol(format!("Read DateList: {e}"))))?;
        schedule::decode_date_list(&val)
            .ok_or_else(|| BridgeError::Protocol("Failed to decode DateList".into()))
    }

    /// Read the exception schedule from a BACnet Schedule object.
    pub async fn read_exception_schedule(
        &self,
        device_instance: u32,
        schedule_instance: u32,
    ) -> Result<ClientDataValue, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let object_id = ObjectId::new(ObjectType::Schedule, schedule_instance);
        with_client!(tc, |c| c
            .read_property(dev.address, object_id, PropertyId::ExceptionSchedule)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "Read ExceptionSchedule: {e}"
            ))))
    }

    // -----------------------------------------------------------------------
    // Alarm/Event services
    // -----------------------------------------------------------------------

    /// Acknowledge an alarm on a remote BACnet device.
    pub async fn acknowledge_alarm(
        &self,
        device_instance: u32,
        object_id: ObjectId,
        event_state: EventState,
        event_timestamp: TimeStamp,
        source: &str,
    ) -> Result<(), BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let (date, time) = now_bacnet_utc();
        let request = AcknowledgeAlarmRequest {
            acknowledging_process_id: 0,
            event_object_id: object_id,
            event_state_acknowledged: event_state,
            event_time_stamp: event_timestamp,
            acknowledgment_source: source,
            time_of_acknowledgment: TimeStamp::DateTime { date, time },
            invoke_id: 0, // will be overwritten by client
        };
        with_client!(tc, |c| c
            .acknowledge_alarm(dev.address, request)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "AcknowledgeAlarm failed: {e}"
            ))))?;
        Ok(())
    }

    /// Get alarm summary from a remote BACnet device.
    pub async fn get_alarm_summary(
        &self,
        device_instance: u32,
    ) -> Result<Vec<AlarmSummaryItem>, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        with_client!(tc, |c| c.get_alarm_summary(dev.address).await.map_err(
            |e| BridgeError::Protocol(format!("GetAlarmSummary failed: {e}"))
        ))
    }

    /// Get enrollment summary from a remote BACnet device.
    pub async fn get_enrollment_summary(
        &self,
        device_instance: u32,
    ) -> Result<Vec<EnrollmentSummaryItem>, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        with_client!(tc, |c| c.get_enrollment_summary(dev.address).await.map_err(
            |e| BridgeError::Protocol(format!("GetEnrollmentSummary failed: {e}"))
        ))
    }

    // -----------------------------------------------------------------------
    // Object management
    // -----------------------------------------------------------------------

    /// Create a new object on a remote BACnet device.
    pub async fn create_object(
        &self,
        device_instance: u32,
        object_type: ObjectType,
    ) -> Result<ObjectId, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        with_client!(tc, |c| c
            .create_object_by_type(dev.address, object_type)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "CreateObject failed: {e}"
            ))))
    }

    /// Delete an object from a remote BACnet device.
    pub async fn delete_object(
        &self,
        device_instance: u32,
        object_id: ObjectId,
    ) -> Result<(), BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        with_client!(tc, |c| c
            .delete_object(dev.address, object_id)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "DeleteObject failed: {e}"
            ))))
    }

    // -----------------------------------------------------------------------
    // File operations
    // -----------------------------------------------------------------------

    /// Read bytes from a BACnet File object using stream access.
    pub async fn read_file_stream(
        &self,
        device_instance: u32,
        file_instance: u32,
        start: i32,
        count: u32,
    ) -> Result<AtomicReadFileResult, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let file_id = ObjectId::new(ObjectType::File, file_instance);
        with_client!(tc, |c| c
            .atomic_read_file_stream(dev.address, file_id, start, count)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "AtomicReadFile(stream) failed: {e}"
            ))))
    }

    /// Read records from a BACnet File object using record access.
    pub async fn read_file_record(
        &self,
        device_instance: u32,
        file_instance: u32,
        start: i32,
        count: u32,
    ) -> Result<AtomicReadFileResult, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let file_id = ObjectId::new(ObjectType::File, file_instance);
        with_client!(tc, |c| c
            .atomic_read_file_record(dev.address, file_id, start, count)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "AtomicReadFile(record) failed: {e}"
            ))))
    }

    /// Write bytes to a BACnet File object using stream access.
    pub async fn write_file_stream(
        &self,
        device_instance: u32,
        file_instance: u32,
        start: i32,
        data: &[u8],
    ) -> Result<AtomicWriteFileResult, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let file_id = ObjectId::new(ObjectType::File, file_instance);
        with_client!(tc, |c| c
            .atomic_write_file_stream(dev.address, file_id, start, data)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "AtomicWriteFile(stream) failed: {e}"
            ))))
    }

    /// Write records to a BACnet File object using record access.
    pub async fn write_file_record(
        &self,
        device_instance: u32,
        file_instance: u32,
        start: i32,
        records: &[&[u8]],
    ) -> Result<AtomicWriteFileResult, BridgeError> {
        let tc = self.require_transport()?;
        let dev = self.find_device(device_instance)?;
        let file_id = ObjectId::new(ObjectType::File, file_instance);
        with_client!(tc, |c| c
            .atomic_write_file_record(dev.address, file_id, start, records)
            .await
            .map_err(|e| BridgeError::Protocol(format!(
                "AtomicWriteFile(record) failed: {e}"
            ))))
    }
}
