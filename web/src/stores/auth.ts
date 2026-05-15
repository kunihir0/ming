import { defineStore } from 'pinia';
import { ref } from 'vue';

export const useAuthStore = defineStore('auth', () => {
  const user = ref<any>(null);
  const isAuthenticated = ref(false);
  const isInitializing = ref(true);

  const fetchUser = async () => {
    try {
      const res = await fetch('/api/auth/me');
      if (res.ok) {
        user.value = await res.json();
        isAuthenticated.value = true;
      } else {
        user.value = null;
        isAuthenticated.value = false;
      }
    } catch (e) {
      user.value = null;
      isAuthenticated.value = false;
    } finally {
      isInitializing.value = false;
    }
  };

  const logout = async () => {
    await fetch('/api/auth/logout', { method: 'POST' });
    user.value = null;
    isAuthenticated.value = false;
    window.location.href = '/api/auth/discord/login';
  };

  return { user, isAuthenticated, isInitializing, fetchUser, logout };
});