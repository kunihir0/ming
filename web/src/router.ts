import { createRouter, createWebHistory } from 'vue-router'
import Dashboard from './views/Dashboard.vue'
import LinkRustPlus from './views/LinkRustPlus.vue'

const routes = [
  {
    path: '/',
    name: 'Dashboard',
    component: Dashboard,
  },
  {
    path: '/link',
    name: 'LinkRustPlus',
    component: LinkRustPlus,
  },
]

const router = createRouter({
  history: createWebHistory(),
  routes,
})

export default router