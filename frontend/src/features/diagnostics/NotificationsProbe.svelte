<script lang="ts">
  import {
    listenToNotifications,
    notificationPermissionGranted,
    requestNotificationPermission,
    scheduleProbeNotification,
    sendProbeNotification,
    type NotificationEvent,
  } from '../../api/nativeDiagnostics'
  import { useServices } from '../../app/context'
  import ProbeLog, { logEntry, type LogEntry } from './ProbeLog.svelte'

  const SCHEDULE_DELAY_SECONDS = 10

  const { i18n } = useServices()
  let busy = $state(false)
  let entries = $state<LogEntry[]>([])

  const log = (text: string, failed = false) => {
    entries = [logEntry(text, failed), ...entries]
  }
  const content = () => ({ title: i18n.t('diagnostics.notificationTitle'), body: i18n.t('diagnostics.notificationBody') })

  $effect(() => {
    let stop: (() => void) | null = null
    let disposed = false
    const onEvent = (event: NotificationEvent) =>
      log(i18n.t(event.kind === 'tapped' ? 'diagnostics.notificationTapped' : 'diagnostics.notificationReceived', { id: event.id ?? '?', extra: event.extra }))
    listenToNotifications(onEvent).then(
      (unlisten) => (disposed ? unlisten() : (stop = unlisten)),
      (error: unknown) => log(i18n.t('diagnostics.actionFailed', { action: 'listen', error: String(error) }), true),
    )
    return () => {
      disposed = true
      stop?.()
    }
  })

  async function run(action: string, task: () => Promise<string>) {
    busy = true
    try {
      log(await task())
    } catch (error) {
      log(i18n.t('diagnostics.actionFailed', { action, error: String(error) }), true)
    } finally {
      busy = false
    }
  }

  const checkPermission = () =>
    run('isPermissionGranted', async () => i18n.t('diagnostics.permissionGranted', { value: String(await notificationPermissionGranted()) }))
  const requestPermission = () =>
    run('requestPermission', async () => i18n.t('diagnostics.permissionRequested', { value: await requestNotificationPermission() }))
  const sendNow = () =>
    run('notify', async () => {
      await sendProbeNotification(content())
      return i18n.t('diagnostics.notificationSent')
    })
  const schedule = () =>
    run('schedule', async () => {
      const pending = await scheduleProbeNotification(content(), SCHEDULE_DELAY_SECONDS)
      return i18n.t('diagnostics.notificationScheduled', { seconds: SCHEDULE_DELAY_SECONDS, pending: String(pending) })
    })
</script>

<div class="row">
  <button type="button" class="btn" disabled={busy} onclick={checkPermission}>{i18n.t('diagnostics.checkPermission')}</button>
  <button type="button" class="btn" disabled={busy} onclick={requestPermission}>{i18n.t('diagnostics.requestPermission')}</button>
  <button type="button" class="btn" disabled={busy} onclick={sendNow}>{i18n.t('diagnostics.sendNow')}</button>
  <button type="button" class="btn btn-primary" disabled={busy} onclick={schedule}>{i18n.t('diagnostics.scheduleIn', { seconds: SCHEDULE_DELAY_SECONDS })}</button>
</div>
<ProbeLog {entries} />
