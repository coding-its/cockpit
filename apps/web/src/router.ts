import { createRouter, createWebHistory } from 'vue-router'
import { useAppStore } from './stores/app'

const router = createRouter({
  history: createWebHistory(),
  routes: [
    {
      path: '/login',
      name: 'login',
      component: () => import('./views/LoginView.vue'),
      meta: { title: 'Anmelden', public: true },
    },
    {
      path: '/einstellungen',
      name: 'settings',
      component: () => import('./views/SettingsView.vue'),
      meta: { title: 'Einstellungen', adminOnly: true },
    },
    {
      // Alte Adresse bleibt gültig – Lesezeichen und Links sollen weiter greifen.
      path: '/benutzer',
      redirect: { path: '/einstellungen' },
    },
    {
      path: '/',
      name: 'dashboard',
      component: () => import('./views/DashboardView.vue'),
      meta: { title: 'Übersicht' },
    },
    {
      path: '/kunden',
      name: 'customers',
      component: () => import('./views/CustomersView.vue'),
      meta: { title: 'Kunden' },
    },
    {
      path: '/kunden/:id',
      name: 'customer',
      component: () => import('./views/CustomerDetailView.vue'),
      meta: { title: 'Kundendetails' },
    },
    {
      path: '/kunden/:id/abrechnung',
      name: 'statement',
      component: () => import('./views/StatementView.vue'),
      meta: { title: 'Abrechnung' },
    },
    {
      path: '/einsaetze',
      name: 'tickets',
      component: () => import('./views/TicketsView.vue'),
      meta: { title: 'Einsätze' },
    },
    {
      path: '/websites',
      name: 'websites',
      component: () => import('./views/WebsitesView.vue'),
      meta: { title: 'Websites' },
    },
    {
      path: '/websites/:id',
      name: 'webclient',
      component: () => import('./views/WebClientView.vue'),
      meta: { title: 'Web-Kunde' },
    },
    {
      path: '/websites/:id/abrechnung',
      name: 'web-statement',
      component: () => import('./views/StatementView.vue'),
      meta: { title: 'Abrechnung' },
    },
    {
      path: '/wartungen',
      name: 'runs',
      component: () => import('./views/RunsView.vue'),
      meta: { title: 'Wartungen' },
    },
    {
      path: '/wartungen/:id',
      name: 'run',
      component: () => import('./views/RunDetailView.vue'),
      meta: { title: 'Wartungslauf' },
    },
    {
      path: '/zeiten',
      name: 'time',
      component: () => import('./views/TimeView.vue'),
      meta: { title: 'Zeiten & Abrechnung' },
    },
    {
      path: '/bericht',
      name: 'report',
      component: () => import('./views/ReportView.vue'),
      meta: { title: 'Monatsbericht' },
    },
    {
      path: '/integrationen',
      name: 'integrations',
      component: () => import('./views/IntegrationsView.vue'),
      meta: { title: 'Integrationen' },
    },
  ],
  scrollBehavior: () => ({ top: 0 }),
})

router.beforeEach(async (to) => {
  const store = useAppStore()

  if (!store.sessionChecked) {
    await store.loadSession()
  }

  if (to.meta.public) {
    return true
  }

  if (!store.user) {
    return { path: '/login', query: { redirect: to.fullPath } }
  }

  if (store.mustChangePassword && to.name !== 'login') {
    return { path: '/login' }
  }

  if (to.meta.adminOnly && !store.isAdmin) {
    return { path: '/' }
  }

  return true
})

export default router
