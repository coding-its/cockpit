import { defineStore } from 'pinia'
import { api } from '../api'
import type { ActiveTimer, CurrentUser, IntegrationStatus, TimerStopResponse } from '../types'

export type ToastKind = 'success' | 'error' | 'info'

export interface Toast {
  id: number
  message: string
  kind: ToastKind
}

export const useAppStore = defineStore('app', {
  state: () => ({
    activeTimer: null as ActiveTimer | null,
    integration: null as IntegrationStatus | null,
    user: null as CurrentUser | null,
    sessionChecked: false,
    toasts: [] as Toast[],
    syncing: false,
    /// Eben gestoppter Eintrag – der Kommentar-Dialog hängt sich daran.
    pendingComment: null as TimerStopResponse | null,
  }),
  getters: {
    isLive: (state) => state.integration?.ninjaone_mode === 'live',
    mustChangePassword: (state) => state.user?.must_change_password === true,
    isAdmin: (state) => state.user?.role === 'admin',
    canWrite: (state) => !!state.user && state.user.role !== 'viewer',
  },
  actions: {
    async loadTimer() {
      try {
        this.activeTimer = await api.timer()
      } catch {
        this.activeTimer = null
      }
    },
    setUser(user: CurrentUser | null) {
      this.user = user
      this.sessionChecked = true
    },
    clearSession() {
      this.user = null
      this.activeTimer = null
      this.integration = null
      this.sessionChecked = true
    },
    async loadSession() {
      try {
        const result = await api.session()
        this.user = result.user
      } catch {
        this.user = null
      } finally {
        this.sessionChecked = true
      }
    },
    async loadIntegration() {
      try {
        this.integration = await api.integrations()
      } catch {
        this.integration = null
      }
    },
    async syncNinjaOne() {
      const result = await api.sync()
      await this.loadIntegration()
      return result
    },
    async startTimer(taskId: string) {
      const timer = await api.startTimer(taskId)
      this.activeTimer = timer
      this.toast(`Timer läuft: ${timer?.activity ?? 'Aufgabe'}`, 'success')
    },
    async stopTimer() {
      const result = await api.stopTimer()
      this.activeTimer = null
      if (result) {
        // Kein Toast: der Kommentar-Dialog zeigt Aktivität und Dauer bereits.
        this.pendingComment = result
      }
    },
    toast(message: string, kind: ToastKind = 'info') {
      const id = Date.now() + Math.random()
      this.toasts.push({ id, message, kind })
      window.setTimeout(() => this.dismiss(id), 4800)
    },
    dismiss(id: number) {
      this.toasts = this.toasts.filter((toast) => toast.id !== id)
    },
  },
})
