#!/usr/bin/env node
/**
 * Signierter Testaufruf für den Wartungscockpit-Agent.
 *
 * Benötigte Umgebungsvariablen:
 *   WP_URL           z. B. https://www.example.de
 *   WP_USER          WordPress-Benutzername (Administrator)
 *   WP_APP_PASSWORD  Application Password (mit Leerzeichen, wie angezeigt)
 *   WP_AGENT_SECRET  Wert von WC_MAINT_SHARED_SECRET
 *
 * Beispiele:
 *   node test-request.mjs health
 *   node test-request.mjs inventory
 *   node test-request.mjs update-plugins            # Dry-Run
 *   node test-request.mjs update-plugins --apply --backup
 *   node test-request.mjs update-core --apply --backup
 */

import crypto from 'node:crypto'

const baseUrl = process.env.WP_URL?.replace(/\/+$/, '')
const user = process.env.WP_USER
const appPassword = process.env.WP_APP_PASSWORD
const secret = process.env.WP_AGENT_SECRET

if (!baseUrl || !user || !appPassword || !secret) {
  console.error('WP_URL, WP_USER, WP_APP_PASSWORD und WP_AGENT_SECRET müssen gesetzt sein.')
  process.exit(1)
}

const [, , endpoint = 'health', ...flags] = process.argv
const apply = flags.includes('--apply')
const backup = flags.includes('--backup')

const routes = {
  health: { method: 'GET', path: '/health' },
  inventory: { method: 'GET', path: '/inventory?refresh=true' },
  log: { method: 'GET', path: '/log' },
  'update-plugins': {
    method: 'POST',
    path: '/updates/plugins',
    body: { dry_run: !apply, backup_confirmed: backup },
  },
  'update-core': {
    method: 'POST',
    path: '/updates/core',
    body: { dry_run: !apply, backup_confirmed: backup },
  },
}

const route = routes[endpoint]
if (!route) {
  console.error(`Unbekannter Endpunkt: ${endpoint}`)
  console.error(`Verfügbar: ${Object.keys(routes).join(', ')}`)
  process.exit(1)
}

const rawBody = route.body ? JSON.stringify(route.body) : ''
const timestamp = Math.floor(Date.now() / 1000).toString()
const signature = crypto
  .createHmac('sha256', secret)
  .update(`${timestamp}.${rawBody}`)
  .digest('hex')

const authorization = Buffer.from(`${user}:${appPassword}`).toString('base64')

const response = await fetch(`${baseUrl}/wp-json/maintenance/v1${route.path}`, {
  method: route.method,
  headers: {
    Authorization: `Basic ${authorization}`,
    'Content-Type': 'application/json',
    'X-Maintenance-Timestamp': timestamp,
    'X-Maintenance-Signature': signature,
  },
  body: route.method === 'POST' ? rawBody : undefined,
})

const text = await response.text()
console.log(`HTTP ${response.status} ${response.statusText}`)
try {
  console.log(JSON.stringify(JSON.parse(text), null, 2))
} catch {
  console.log(text)
}

if (!response.ok) {
  process.exit(1)
}
