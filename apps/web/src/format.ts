export function formatDate(value?: string | null): string {
  if (!value) return '–'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleDateString('de-DE', { day: '2-digit', month: '2-digit', year: 'numeric' })
}

export function formatDateTime(value?: string | null): string {
  if (!value) return '–'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString('de-DE', {
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

export function formatDateLong(value?: string | null): string {
  if (!value) return '–'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleDateString('de-DE', {
    weekday: 'long',
    day: '2-digit',
    month: 'long',
    year: 'numeric',
  })
}

export function todayLong(): string {
  return formatDateLong(new Date().toISOString())
}

export function formatHours(value: number): string {
  const formatted = value.toLocaleString('de-DE', {
    minimumFractionDigits: value % 1 === 0 ? 0 : 1,
    maximumFractionDigits: 2,
  })
  return `${formatted} h`
}

/**
 * Dauer in Stunden, Minuten und Sekunden. Die Sekunden bleiben sichtbar,
 * solange sie nicht null sind – sonst ließe sich der Betrag daneben nicht
 * nachrechnen (1 h 30 min wäre 189,08 €, erfasst waren 190,59 €).
 */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds))
  const hours = Math.floor(total / 3600)
  const minutes = Math.floor((total % 3600) / 60)
  const rest = total % 60

  if (hours > 0) {
    const base = `${hours} h ${String(minutes).padStart(2, '0')} min`
    return rest > 0 ? `${base} ${String(rest).padStart(2, '0')} s` : base
  }
  if (minutes > 0) return `${minutes} min ${String(rest).padStart(2, '0')} s`
  return `${rest} s`
}

export function formatTimer(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds))
  const hours = Math.floor(total / 3600)
  const minutes = Math.floor((total % 3600) / 60)
  const rest = total % 60
  return [hours, minutes, rest].map((part) => String(part).padStart(2, '0')).join(':')
}

export function currency(value: number): string {
  return new Intl.NumberFormat('de-DE', {
    style: 'currency',
    currency: 'EUR',
    maximumFractionDigits: 2,
  }).format(value)
}

export function initials(name: string): string {
  return name
    .replace(/[^\p{L}\s-]/gu, '')
    .split(/[\s-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('')
}

export function currentMonth(): string {
  const now = new Date()
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, '0')}`
}

export function monthLabel(month: string): string {
  const [year, monthNumber] = month.split('-').map(Number)
  if (!year || !monthNumber) return month
  return new Date(year, monthNumber - 1, 1).toLocaleDateString('de-DE', {
    month: 'long',
    year: 'numeric',
  })
}

export function shiftMonth(month: string, delta: number): string {
  const [year, monthNumber] = month.split('-').map(Number)
  const date = new Date(year, monthNumber - 1 + delta, 1)
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}`
}

export function relativeTime(value: string): string {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  const diffSeconds = Math.round((date.getTime() - Date.now()) / 1000)
  const formatter = new Intl.RelativeTimeFormat('de', { numeric: 'auto' })

  const ranges: Array<[Intl.RelativeTimeFormatUnit, number]> = [
    ['year', 60 * 60 * 24 * 365],
    ['month', 60 * 60 * 24 * 30],
    ['day', 60 * 60 * 24],
    ['hour', 60 * 60],
    ['minute', 60],
  ]

  for (const [unit, seconds] of ranges) {
    if (Math.abs(diffSeconds) >= seconds) {
      return formatter.format(Math.round(diffSeconds / seconds), unit)
    }
  }

  return 'gerade eben'
}

export function greeting(): string {
  const hour = new Date().getHours()
  if (hour < 11) return 'Guten Morgen'
  if (hour < 18) return 'Guten Tag'
  return 'Guten Abend'
}

export function runStatusLabel(status: string): string {
  return (
    {
      planned: 'Geplant',
      in_progress: 'Läuft',
      done: 'Abgeschlossen',
      blocked: 'Blockiert',
    }[status] ?? status
  )
}

export function taskStatusLabel(status: string): string {
  return (
    {
      open: 'Offen',
      running: 'Läuft',
      done: 'Erledigt',
      failed: 'Fehler',
      skipped: 'Übersprungen',
    }[status] ?? status
  )
}

export function riskLabel(risk: string): string {
  return (
    {
      low: 'Risiko niedrig',
      medium: 'Risiko mittel',
      high: 'Risiko hoch',
    }[risk] ?? risk
  )
}

export function siteStatusLabel(status: string): string {
  return (
    {
      healthy: 'Stabil',
      warning: 'Beobachten',
      critical: 'Kritisch',
      unknown: 'Ungeprüft',
    }[status] ?? status
  )
}

export function sourceLabel(source: string): string {
  return (
    {
      timer: 'Timer',
      manual: 'Manuell',
      machine: 'Maschinenzeit',
    }[source] ?? source
  )
}

export function taskKindLabel(kind: string): string {
  return (
    {
      NINJA_PATCH: 'NinjaOne Patch',
      NINJA_SCRIPT: 'NinjaOne Script',
      WP_PLUGIN_UPDATE: 'WordPress Plugins',
      WP_CORE_UPDATE: 'WordPress Core',
      WP_HEALTH: 'WordPress Health',
      MANUAL: 'Manuell',
    }[kind] ?? kind
  )
}

export function priorityLabel(priority: string): string {
  return (
    {
      low: 'Niedrig',
      normal: 'Normal',
      high: 'Hoch',
      urgent: 'Dringend',
    }[priority] ?? priority
  )
}

export function runKindLabel(kind: string): string {
  return kind === 'ticket' ? 'Einsatz' : 'Wartung'
}

export function ticketStatusLabel(status: string): string {
  return (
    {
      planned: 'Neu',
      in_progress: 'In Arbeit',
      blocked: 'Wartet',
      done: 'Erledigt',
    }[status] ?? status
  )
}

export function ageDays(value: string): number {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return 0
  return Math.max(0, Math.floor((Date.now() - date.getTime()) / 86_400_000))
}

/** Deep-Link zur Geräteseite in der NinjaOne-Oberfläche. */
export function ninjaDeviceUrl(baseUrl: string, deviceId: number | null): string | null {
  if (!deviceId) return null
  const base = baseUrl.replace(/\/$/, '')
  return `${base}/#/deviceDashboard/${deviceId}/overview`
}

/** Deep-Link, der TeamViewer mit der Geräte-ID öffnet (Schema seit v10). */
export function teamViewerUrl(teamviewerId: string | null): string | null {
  const digits = (teamviewerId ?? '').replace(/\D/g, '')
  if (digits.length < 6) return null
  return `teamviewer10://control?device=${digits}`
}

export function priorityTone(priority: string): 'red' | 'amber' | 'teal' | 'muted' {
  if (priority === 'urgent') return 'red'
  if (priority === 'high') return 'amber'
  if (priority === 'normal') return 'teal'
  return 'muted'
}
