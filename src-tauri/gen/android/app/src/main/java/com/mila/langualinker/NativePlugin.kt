package com.mila.langualinker

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.webkit.WebView
import androidx.appcompat.app.AppCompatActivity
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import org.json.JSONObject
import java.util.Locale

/**
 * Native adapters of plan Phase 15 (ADR-20), registered from Rust (`src-tauri/src/native.rs`):
 * - text to speech through the system `TextToSpeech` engine (Android WebView has no Web Speech
 *   API, ADR-12), with transient audio focus and a stop when the app leaves the foreground;
 * - the deep link a reminder carries: `tauri-plugin-notification` puts the notification JSON on
 *   the launch intent, and its own tap event is lost on a cold start (no JS listener yet), so the
 *   URL is kept here until the WebView asks for it.
 */
@TauriPlugin
class NativePlugin(private val activity: Activity) : Plugin(activity), TextToSpeech.OnInitListener {
  private var tts: TextToSpeech? = null
  private var engineState = EngineState.STARTING
  private val waiting = mutableListOf<Invoke>()
  private var launchUrl: String? = null
  private val audio = activity.getSystemService(Context.AUDIO_SERVICE) as AudioManager
  private var focus: AudioFocusRequest? = null

  private enum class EngineState { STARTING, READY, MISSING }

  override fun load(webView: WebView) {
    super.load(webView)
    tts = TextToSpeech(activity.applicationContext, this)
    activity.intent?.let { capture(it) }
  }

  override fun onInit(status: Int) {
    val engine = tts
    engineState = if (status == TextToSpeech.SUCCESS && engine != null) EngineState.READY else EngineState.MISSING
    engine?.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
      override fun onStart(utteranceId: String?) {}
      override fun onDone(utteranceId: String?) = releaseFocus()
      @Deprecated("Deprecated in Java")
      override fun onError(utteranceId: String?) = releaseFocus()
      override fun onStop(utteranceId: String?, interrupted: Boolean) = releaseFocus()
    })
    val pending = waiting.toList()
    waiting.clear()
    pending.forEach { answerStatus(it) }
  }

  override fun onNewIntent(intent: Intent) {
    capture(intent)
  }

  override fun onPause(activity: AppCompatActivity) {
    // Speech never continues in the background (plan Phase 15 step 9).
    tts?.stop()
    releaseFocus()
  }

  override fun onDestroy(activity: AppCompatActivity) {
    tts?.shutdown()
    tts = null
  }

  private fun capture(intent: Intent) {
    val json = intent.getStringExtra(NOTIFICATION_OBJECT_KEY)
    val url = json?.let {
      try {
        JSONObject(it).optJSONObject("extra")?.optString("url")
      } catch (_: Exception) {
        null
      }
    }
    // Only our own scheme; the WebView validates the route again.
    if (url != null && url.startsWith("langualinker://")) {
      launchUrl = url
      return
    }
    // tauri-plugin-notification 2.5.1 never fills the notification JSON of the tap intent
    // (`sourceJson` stays null), only its id: the reminder ids (Rust `REMINDER_ID_BASE`) open home.
    val id = intent.getIntExtra(NOTIFICATION_ID_KEY, Int.MIN_VALUE)
    if (id in REMINDER_IDS) launchUrl = REMINDER_URL
  }

  @Command
  fun speechStatus(invoke: Invoke) {
    if (engineState == EngineState.STARTING) {
      waiting.add(invoke)
    } else {
      answerStatus(invoke)
    }
  }

  private fun answerStatus(invoke: Invoke) {
    val result = JSObject()
    val engine = tts
    val ready = engineState == EngineState.READY && engine != null
    result.put("engine", ready)
    val lang = invoke.getArgs().getString("lang", "")
    if (ready && !lang.isNullOrEmpty()) {
      val locale = Locale.forLanguageTag(lang)
      val available = try {
        engine!!.isLanguageAvailable(locale) >= TextToSpeech.LANG_AVAILABLE
      } catch (_: Exception) {
        false
      }
      val offline = available && try {
        engine!!.voices.orEmpty().any { voice ->
          voice.locale.language == locale.language &&
            !voice.isNetworkConnectionRequired &&
            !voice.features.contains(TextToSpeech.Engine.KEY_FEATURE_NOT_INSTALLED)
        }
      } catch (_: Exception) {
        false
      }
      result.put("voice", available)
      result.put("offline", offline)
    } else {
      result.put("voice", false)
      result.put("offline", false)
    }
    invoke.resolve(result)
  }

  @Command
  fun speak(invoke: Invoke) {
    val args = invoke.getArgs()
    val engine = tts
    if (engineState != EngineState.READY || engine == null) {
      invoke.reject("No speech engine", "UNSUPPORTED")
      return
    }
    val text = args.getString("text", "") ?: ""
    val lang = args.getString("lang", "") ?: ""
    val rate = args.optDouble("rate", 1.0).coerceIn(0.5, 1.5).toFloat()
    val result = engine.setLanguage(Locale.forLanguageTag(lang))
    if (result == TextToSpeech.LANG_MISSING_DATA || result == TextToSpeech.LANG_NOT_SUPPORTED) {
      invoke.reject("No voice for $lang", "NO_VOICE")
      return
    }
    engine.setSpeechRate(rate)
    requestFocus()
    engine.speak(text.take(MAX_TEXT), TextToSpeech.QUEUE_FLUSH, null, UTTERANCE_ID)
    invoke.resolve()
  }

  @Command
  fun stopSpeech(invoke: Invoke) {
    tts?.stop()
    releaseFocus()
    invoke.resolve()
  }

  /** The URL of the reminder that opened the app, once. */
  @Command
  fun takeLaunchUrl(invoke: Invoke) {
    val result = JSObject()
    result.put("url", launchUrl)
    launchUrl = null
    invoke.resolve(result)
  }

  private fun requestFocus() {
    val request = focus ?: AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
      .setAudioAttributes(
        AudioAttributes.Builder()
          .setUsage(AudioAttributes.USAGE_ASSISTANCE_ACCESSIBILITY)
          .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
          .build()
      )
      // Another app takes the audio (a call, a navigation prompt): speech stops.
      .setOnAudioFocusChangeListener { change ->
        if (change == AudioManager.AUDIOFOCUS_LOSS || change == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT) {
          tts?.stop()
        }
      }
      .build()
    focus = request
    audio.requestAudioFocus(request)
  }

  private fun releaseFocus() {
    focus?.let { audio.abandonAudioFocusRequest(it) }
  }

  companion object {
    /** Extra of `tauri-plugin-notification` (TauriNotificationManager.NOTIFICATION_OBJ_INTENT_KEY). */
    private const val NOTIFICATION_OBJECT_KEY = "LocalNotficationObject"
    /** TauriNotificationManager.NOTIFICATION_INTENT_KEY. */
    private const val NOTIFICATION_ID_KEY = "NotificationId"
    /** `src-tauri/src/reminders.rs`: REMINDER_ID_BASE until BASE + SPAN, and REMINDER_URL. */
    private val REMINDER_IDS = 15_000 until 15_100
    private const val REMINDER_URL = "langualinker://app/home"
    private const val UTTERANCE_ID = "langualinker-speech"
    private const val MAX_TEXT = 1000
  }
}
