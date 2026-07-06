package com.dreamnarrativeengine.app

import android.Manifest
import android.content.ContentUris
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.provider.CalendarContract
import android.util.Log
import androidx.core.app.ActivityCompat
import org.json.JSONObject
import java.util.TimeZone
import java.util.concurrent.ConcurrentHashMap

class ScheduledNotificationReceiver {
    companion object {
        private const val TAG = "CloudDreamCalendar"
        private const val APP_NAME = "\u4e91\u6735\u68a6\u5883"
        private const val PREFS_NAME = "scheduled_calendar_reminders"
        private const val KEY_EVENT_ID_PREFIX = "event_id:"
        private const val DEFAULT_EVENT_DURATION_MS = 30L * 60L * 1000L
        private const val DEFAULT_REMINDER_MINUTES = 0
        private const val LOCAL_CALENDAR_NAME = "cloud_dream_reminders"

        @Volatile
        private var appContext: Context? = null

        private val inMemoryEvents = ConcurrentHashMap<String, Long>()

        @JvmStatic
        fun initialize(context: Context) {
            appContext = context.applicationContext
            loadStoredEventIds(context.applicationContext)
        }

        @JvmStatic
        fun scheduleNotificationFromRust(
            notificationId: String,
            title: String,
            body: String,
            channelId: String,
            delayMillis: Long
        ): String {
            val context = appContext ?: return scheduleResult(
                ok = false,
                notificationId = notificationId,
                error = "Android calendar context is not ready"
            ).toString()
            val startAt = System.currentTimeMillis() + delayMillis.coerceAtLeast(0L)
            return try {
                ensureCalendarPermission(context)
                val calendarId = findWritableCalendarId(context)
                    ?: ensureLocalCalendar(context)
                    ?: return scheduleResult(
                        ok = false,
                        notificationId = notificationId,
                        triggerAt = startAt,
                        error = "No writable Android calendar is available"
                    ).toString()
                val existingEventId = loadStoredEventId(context, notificationId)
                if (existingEventId != null) {
                    deleteCalendarEvent(context, existingEventId)
                    removeStoredEventId(context, notificationId)
                }
                val eventId = insertCalendarEvent(
                    context = context,
                    calendarId = calendarId,
                    notificationId = notificationId,
                    title = title.ifBlank { APP_NAME },
                    body = body,
                    startAt = startAt,
                )
                insertCalendarReminder(context, eventId, DEFAULT_REMINDER_MINUTES)
                storeEventId(context, notificationId, eventId)
                Log.i(
                    TAG,
                    "Created calendar reminder id=$notificationId eventId=$eventId calendarId=$calendarId triggerAt=$startAt manufacturer=${Build.MANUFACTURER}"
                )
                scheduleResult(
                    ok = true,
                    notificationId = notificationId,
                    triggerAt = startAt,
                    requestCode = eventId.toInt(),
                    channelId = channelId,
                    calendarEventCreated = true,
                    calendarEventId = eventId,
                    calendarId = calendarId,
                    calendarReminderMinutes = DEFAULT_REMINDER_MINUTES,
                    manufacturer = Build.MANUFACTURER,
                    model = Build.MODEL,
                    sdkInt = Build.VERSION.SDK_INT
                ).toString()
            } catch (error: Throwable) {
                Log.e(TAG, "Failed to create calendar reminder: $notificationId", error)
                scheduleResult(
                    ok = false,
                    notificationId = notificationId,
                    triggerAt = startAt,
                    error = error.message ?: error.javaClass.name
                ).toString()
            }
        }

        @JvmStatic
        fun cancelNotificationFromRust(notificationId: String): String {
            val context = appContext ?: return scheduleResult(
                ok = false,
                notificationId = notificationId,
                error = "Android calendar context is not ready"
            ).toString()
            return try {
                ensureCalendarPermission(context)
                val eventId = loadStoredEventId(context, notificationId)
                if (eventId != null) {
                    deleteCalendarEvent(context, eventId)
                    removeStoredEventId(context, notificationId)
                }
                scheduleResult(
                    ok = true,
                    notificationId = notificationId,
                    calendarEventId = eventId
                ).toString()
            } catch (error: Throwable) {
                Log.e(TAG, "Failed to cancel calendar reminder: $notificationId", error)
                scheduleResult(
                    ok = false,
                    notificationId = notificationId,
                    error = error.message ?: error.javaClass.name
                ).toString()
            }
        }

        private fun ensureCalendarPermission(context: Context) {
            val readGranted = ActivityCompat.checkSelfPermission(
                context,
                Manifest.permission.READ_CALENDAR
            ) == PackageManager.PERMISSION_GRANTED
            val writeGranted = ActivityCompat.checkSelfPermission(
                context,
                Manifest.permission.WRITE_CALENDAR
            ) == PackageManager.PERMISSION_GRANTED
            if (!readGranted || !writeGranted) {
                throw SecurityException("Calendar permission is required to create system calendar reminders")
            }
        }

        private fun findWritableCalendarId(context: Context): Long? {
            val projection = arrayOf(
                CalendarContract.Calendars._ID,
                CalendarContract.Calendars.CALENDAR_ACCESS_LEVEL,
                CalendarContract.Calendars.VISIBLE,
                CalendarContract.Calendars.IS_PRIMARY
            )
            context.contentResolver.query(
                CalendarContract.Calendars.CONTENT_URI,
                projection,
                "${CalendarContract.Calendars.CALENDAR_ACCESS_LEVEL} >= ?",
                arrayOf(CalendarContract.Calendars.CAL_ACCESS_CONTRIBUTOR.toString()),
                "${CalendarContract.Calendars.IS_PRIMARY} DESC, ${CalendarContract.Calendars.VISIBLE} DESC"
            )?.use { cursor ->
                while (cursor.moveToNext()) {
                    val id = cursor.getLong(0)
                    if (id > 0L) return id
                }
            }
            return null
        }

        /**
         * 设备上没有可写日历(例如未登录任何 Google/同步账户)时,创建一个应用自有的本地日历。
         * 本地日历(ACCOUNT_TYPE_LOCAL)不依赖任何在线账户,只要用户授予日历权限即可写入,
         * 保证"有权限就能用"。
         */
        private fun ensureLocalCalendar(context: Context): Long? {
            existingLocalCalendarId(context)?.let { return it }
            val values = android.content.ContentValues().apply {
                put(CalendarContract.Calendars.ACCOUNT_NAME, APP_NAME)
                put(CalendarContract.Calendars.ACCOUNT_TYPE, CalendarContract.ACCOUNT_TYPE_LOCAL)
                put(CalendarContract.Calendars.NAME, LOCAL_CALENDAR_NAME)
                put(CalendarContract.Calendars.CALENDAR_DISPLAY_NAME, APP_NAME)
                put(
                    CalendarContract.Calendars.CALENDAR_ACCESS_LEVEL,
                    CalendarContract.Calendars.CAL_ACCESS_OWNER
                )
                put(CalendarContract.Calendars.OWNER_ACCOUNT, APP_NAME)
                put(CalendarContract.Calendars.VISIBLE, 1)
                put(CalendarContract.Calendars.SYNC_EVENTS, 1)
            }
            val uri = CalendarContract.Calendars.CONTENT_URI.buildUpon()
                .appendQueryParameter(CalendarContract.CALLER_IS_SYNCADAPTER, "true")
                .appendQueryParameter(CalendarContract.Calendars.ACCOUNT_NAME, APP_NAME)
                .appendQueryParameter(
                    CalendarContract.Calendars.ACCOUNT_TYPE,
                    CalendarContract.ACCOUNT_TYPE_LOCAL
                )
                .build()
            val inserted = context.contentResolver.insert(uri, values) ?: return null
            return ContentUris.parseId(inserted).takeIf { it > 0L }
        }

        private fun existingLocalCalendarId(context: Context): Long? {
            context.contentResolver.query(
                CalendarContract.Calendars.CONTENT_URI,
                arrayOf(CalendarContract.Calendars._ID),
                "${CalendarContract.Calendars.ACCOUNT_TYPE} = ? AND ${CalendarContract.Calendars.NAME} = ?",
                arrayOf(CalendarContract.ACCOUNT_TYPE_LOCAL, LOCAL_CALENDAR_NAME),
                null
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val id = cursor.getLong(0)
                    if (id > 0L) return id
                }
            }
            return null
        }

        private fun insertCalendarEvent(
            context: Context,
            calendarId: Long,
            notificationId: String,
            title: String,
            body: String,
            startAt: Long
        ): Long {
            val values = android.content.ContentValues().apply {
                put(CalendarContract.Events.CALENDAR_ID, calendarId)
                put(CalendarContract.Events.TITLE, title.ifBlank { APP_NAME })
                put(CalendarContract.Events.DESCRIPTION, body)
                put(CalendarContract.Events.DTSTART, startAt)
                put(CalendarContract.Events.DTEND, startAt + DEFAULT_EVENT_DURATION_MS)
                put(CalendarContract.Events.EVENT_TIMEZONE, TimeZone.getDefault().id)
                put(CalendarContract.Events.HAS_ALARM, 1)
                put(CalendarContract.Events.CUSTOM_APP_PACKAGE, context.packageName)
                put(CalendarContract.Events.CUSTOM_APP_URI, "cloud-dream://scheduled-notification/$notificationId")
            }
            val uri = context.contentResolver.insert(CalendarContract.Events.CONTENT_URI, values)
                ?: throw IllegalStateException("Calendar provider did not return an event URI")
            return ContentUris.parseId(uri)
        }

        private fun insertCalendarReminder(context: Context, eventId: Long, minutes: Int) {
            val values = android.content.ContentValues().apply {
                put(CalendarContract.Reminders.EVENT_ID, eventId)
                put(CalendarContract.Reminders.MINUTES, minutes)
                put(CalendarContract.Reminders.METHOD, CalendarContract.Reminders.METHOD_ALERT)
            }
            context.contentResolver.insert(CalendarContract.Reminders.CONTENT_URI, values)
                ?: throw IllegalStateException("Calendar provider did not return a reminder URI")
        }

        private fun deleteCalendarEvent(context: Context, eventId: Long) {
            val uri = ContentUris.withAppendedId(CalendarContract.Events.CONTENT_URI, eventId)
            context.contentResolver.delete(uri, null, null)
        }

        private fun storeEventId(context: Context, notificationId: String, eventId: Long) {
            inMemoryEvents[notificationId] = eventId
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit()
                .putLong("$KEY_EVENT_ID_PREFIX$notificationId", eventId)
                .apply()
        }

        private fun loadStoredEventId(context: Context, notificationId: String): Long? {
            inMemoryEvents[notificationId]?.let { return it }
            val prefs = context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            val key = "$KEY_EVENT_ID_PREFIX$notificationId"
            if (!prefs.contains(key)) return null
            return prefs.getLong(key, -1L)
                .takeIf { it > 0L }
                ?.also { inMemoryEvents[notificationId] = it }
        }

        private fun removeStoredEventId(context: Context, notificationId: String) {
            inMemoryEvents.remove(notificationId)
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit()
                .remove("$KEY_EVENT_ID_PREFIX$notificationId")
                .apply()
        }

        private fun loadStoredEventIds(context: Context) {
            context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .all
                .forEach { (key, value) ->
                    if (key.startsWith(KEY_EVENT_ID_PREFIX) && value is Long && value > 0L) {
                        inMemoryEvents[key.removePrefix(KEY_EVENT_ID_PREFIX)] = value
                    }
                }
        }

        private fun scheduleResult(
            ok: Boolean,
            notificationId: String,
            triggerAt: Long? = null,
            requestCode: Int? = null,
            channelId: String? = null,
            calendarEventCreated: Boolean? = null,
            calendarEventId: Long? = null,
            calendarId: Long? = null,
            calendarReminderMinutes: Int? = null,
            manufacturer: String? = null,
            model: String? = null,
            sdkInt: Int? = null,
            error: String? = null
        ): JSONObject {
            val value = JSONObject()
                .put("ok", ok)
                .put("notification_id", notificationId)
                .put("package_name", "com.dreamnarrativeengine.app")
            triggerAt?.let { value.put("trigger_at_ms", it) }
            requestCode?.let { value.put("request_code", it) }
            channelId?.let { value.put("channel_id", it) }
            calendarEventCreated?.let { value.put("calendar_event_created", it) }
            calendarEventId?.let { value.put("calendar_event_id", it) }
            calendarId?.let { value.put("calendar_id", it) }
            calendarReminderMinutes?.let { value.put("calendar_reminder_minutes", it) }
            manufacturer?.let { value.put("manufacturer", it) }
            model?.let { value.put("model", it) }
            sdkInt?.let { value.put("sdk_int", it) }
            error?.let { value.put("error", it) }
            return value
        }
    }
}
