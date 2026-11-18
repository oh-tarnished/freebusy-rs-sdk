// Copyright 2026 Shokki Engineering
// SPDX-License-Identifier: Apache-2.0

//! booking: freebusy end to end with the Rust SDK, against a running server.
//!
//! It sets up a meeting room open 08:00-18:00 (UTC) every day, then, a week
//! from today: holds 10:00-11:00 and books the hold; holds 14:00-15:00 and
//! releases that hold; checks that 10:00 cannot be held twice; cancels the
//! booking; and checks availability after each step. Finally it deletes the
//! room and its calendar.
//!
//! ```sh
//! FREEBUSY_ENDPOINT=localhost:50051 FREEBUSY_INSECURE=true cargo run --example booking
//! ```

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use shokki_freebusy::google_cloud_type::model::{DateTime, Interval, TimeZone};
use shokki_freebusy::wkt::{Duration, Timestamp};
use shokki_freebusy::{
    Availability, AvailablePeriod, Calendar, CalendarTime, CancelReservationRequest,
    CheckAvailabilityRequest, CreateAvailabilityRequest, CreateCalendarRequest, CreateHoldRequest,
    CreateReservationRequest, CreateResourceRequest, DeleteCalendarRequest, DeleteResourceRequest,
    ErrorReason, Freebusy, Hold, HoldState, ReleaseHoldRequest, Reservation, Resource,
    SearchAvailabilityRequest, StructuredData,
};

const HOUR: i64 = 3600;
const DAY: i64 = 24 * HOUR;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    let client = Freebusy::new().await?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let secs = now.as_secs() as i64;
    // Midnight (UTC) a week from today, and a suffix that keeps this run's
    // names apart from earlier runs'.
    let day = secs - secs.rem_euclid(DAY) + 7 * DAY;
    let run = format!("{:x}", now.as_nanos() & 0xffff_ffff);

    let (room, calendar) = create_room(&client, &format!("oak-{run}"), day).await?;
    println!("room {room}, open 08:00-18:00 UTC");

    let morning = window(day + 10 * HOUR, day + 11 * HOUR);
    let afternoon = window(day + 14 * HOUR, day + 15 * HOUR);
    expect_bookable(&client, &room, &morning, true).await?;

    // The room is among the meeting rooms free at 10:00.
    let request = SearchAvailabilityRequest::new()
        .set_window(morning.clone())
        .set_resource_types(["meeting-room"]);
    let mut matches = client
        .availability()
        .search_availability_auto_paging(request);
    let mut found = false;
    while let Some(m) = matches.next().await {
        found |= m?.resource == room;
    }
    if !found {
        return Err(format!("search found no {room} at 10:00").into());
    }
    println!("search: {room} is free at 10:00");

    // Hold 10:00, then book the hold.
    let hold = create_hold(&client, &room, &morning).await?;
    println!("held 10:00-11:00: {}", hold.name);
    expect_bookable(&client, &room, &morning, false).await?;
    let reservation = Reservation::new()
        .set_hold(&hold.name)
        .set_customer(format!("users/jane-{run}"));
    let booking = client
        .reservations()
        .create(CreateReservationRequest::new().set_reservation(reservation))
        .await?;
    println!("booked {} for {}", booking.name, booking.customer);

    // 10:00 cannot be held again.
    let request = CreateHoldRequest::new().set_hold(
        Hold::new()
            .set_window(morning.clone())
            .set_quantity(1)
            .set_candidate_resources([room.clone()]),
    );
    match client.holds().create(request).await {
        Err(err) if err.is_reason(ErrorReason::CapacityExhausted) => {
            println!("a second hold at 10:00 is refused: {err}");
        }
        other => {
            return Err(
                format!("a second hold at 10:00: want CapacityExhausted, got {other:?}").into(),
            );
        }
    }

    // Hold 14:00, then release it: 14:00 is free again.
    let hold = create_hold(&client, &room, &afternoon).await?;
    println!("held 14:00-15:00: {}", hold.name);
    expect_bookable(&client, &room, &afternoon, false).await?;
    let released = client
        .holds()
        .release(
            ReleaseHoldRequest::new()
                .set_name(&hold.name)
                .set_etag(&hold.etag),
        )
        .await?;
    if released.state != HoldState::Released {
        return Err(format!("released hold is {:?}", released.state).into());
    }
    println!("released {}", released.name);
    expect_bookable(&client, &room, &afternoon, true).await?;

    // Cancel the booking: 10:00 is free again.
    let cancelled = client
        .reservations()
        .cancel(
            CancelReservationRequest::new()
                .set_name(&booking.name)
                .set_reason("customer asked")
                .set_etag(&booking.etag),
        )
        .await?;
    println!("cancelled {}", cancelled.name);
    expect_bookable(&client, &room, &morning, true).await?;

    client
        .resources()
        .delete(DeleteResourceRequest::new().set_name(&room))
        .await?;
    client
        .calendars()
        .delete(
            DeleteCalendarRequest::new()
                .set_name(&calendar)
                .set_force(true),
        )
        .await?;
    println!("done");
    Ok(())
}

/// Creates a meeting room with its own calendar, open 08:00-18:00 (UTC)
/// every day from day; returns the room's and the calendar's names.
async fn create_room(client: &Freebusy, id: &str, day: i64) -> Result<(String, String)> {
    let calendar = client
        .calendars()
        .create(
            CreateCalendarRequest::new()
                .set_calendar_id(id)
                .set_calendar(
                    Calendar::new()
                        .set_display_name(format!("Room {id}"))
                        .set_time_zone(TimeZone::new().set_id("UTC")),
                ),
        )
        .await?;
    let opening = CalendarTime::new().set_date_time(utc(day + 8 * HOUR));
    let hours = AvailablePeriod::new()
        .set_start(opening.clone())
        .set_duration(Duration::clamp(10 * HOUR, 0))
        .set_recurrence_rule("FREQ=DAILY");
    client
        .availabilities()
        .create(
            CreateAvailabilityRequest::new()
                .set_parent(&calendar.name)
                .set_availability(
                    Availability::new()
                        .set_summary("Opening hours")
                        .set_start(opening)
                        .set_available_periods([hours]),
                ),
        )
        .await?;
    // freebusy reads a resource's booking policy from this structured data.
    let policy = StructuredData::new()
        .set_schema("https://freebusy.shokki.engineering/schema/booking-profile")
        .set_format_type("application/json")
        .set_text(r#"{"max_occupancy": 1, "min_duration": "PT30M"}"#);
    let room = client
        .resources()
        .create(
            CreateResourceRequest::new()
                .set_resource_id(id)
                .set_resource(
                    Resource::new()
                        .set_display_name(format!("Room {id}"))
                        .set_resource_types(["meeting-room"])
                        .set_calendar(&calendar.name)
                        .set_structured_data([policy]),
                ),
        )
        .await?;
    Ok((room.name, calendar.name))
}

/// Holds room for window.
async fn create_hold(client: &Freebusy, room: &str, window: &Interval) -> Result<Hold> {
    let hold = Hold::new()
        .set_window(window.clone())
        .set_quantity(1)
        .set_candidate_resources([room]);
    // A retry with this request_id after a lost answer gets this hold back
    // instead of taking the room twice.
    let request = CreateHoldRequest::new()
        .set_hold(hold)
        .set_request_id(request_id());
    let hold = client.holds().create(request).await?;
    if hold.state != HoldState::Active {
        return Err(format!("new hold is {:?}", hold.state).into());
    }
    Ok(hold)
}

/// Fails unless room is (or is not) bookable for window.
async fn expect_bookable(
    client: &Freebusy,
    room: &str,
    window: &Interval,
    want: bool,
) -> Result<()> {
    let request = CheckAvailabilityRequest::new()
        .set_resources([room])
        .set_window(window.clone())
        .set_quantity(1);
    let check = client.availability().check_availability(request).await?;
    let from = window.start_time.as_ref().map_or(0, Timestamp::seconds);
    let hour = from.rem_euclid(DAY) / HOUR;
    if check.bookable != want {
        return Err(format!(
            "{room} at {hour:02}:00: bookable is {}, want {want}",
            check.bookable
        )
        .into());
    }
    println!("check: {hour:02}:00 bookable = {want}");
    Ok(())
}

/// The interval [start, end), in seconds since the epoch.
fn window(start: i64, end: i64) -> Interval {
    Interval::new()
        .set_start_time(Timestamp::clamp(start, 0))
        .set_end_time(Timestamp::clamp(end, 0))
}

/// A wall-clock date-time in UTC.
fn utc(secs: i64) -> DateTime {
    let (year, month, day) = civil(secs.div_euclid(DAY));
    let hour = secs.rem_euclid(DAY) / HOUR;
    DateTime::new()
        .set_year(year as i32)
        .set_month(month as i32)
        .set_day(day as i32)
        .set_hours(hour as i32)
        .set_time_zone(TimeZone::new().set_id("UTC"))
}

/// The (year, month, day) of a count of days since 1970-01-01 (Howard
/// Hinnant's civil_from_days).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// A random-enough version 4 UUID for a request_id, from the clock.
fn request_id() -> String {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
        ^ (u128::from(std::process::id()) << 64);
    let n = (n & !(0xf << 76) | (0x4 << 76)) & !(0x3 << 62) | (0x2 << 62);
    let hex = format!("{n:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
