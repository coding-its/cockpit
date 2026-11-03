/// <reference types="vite/client" />

/** Zeitpunkt des Frontend-Builds, von Vite eingesetzt. */
declare const __BUILD_TIME__: string

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
  export default component
}
