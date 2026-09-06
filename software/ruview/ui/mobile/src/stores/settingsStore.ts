import AsyncStorage from '@react-native-async-storage/async-storage';
import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';

export type Theme = 'light' | 'dark' | 'system';

export interface SettingsState {
  serverUrl: string;
  apiToken: string;
  setApiToken: (token: string) => void;
  rssiScanEnabled: boolean;
  theme: Theme;
  alertSoundEnabled: boolean;
  setServerUrl: (url: string) => void;
  setRssiScanEnabled: (value: boolean) => void;
  setTheme: (theme: Theme) => void;
  setAlertSoundEnabled: (value: boolean) => void;
}

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      serverUrl: 'http://localhost:3000',
      apiToken: '',
      setApiToken: (apiToken) => set({ apiToken }),
      rssiScanEnabled: false,
      theme: 'system',
      alertSoundEnabled: true,

      setServerUrl: (url) => {
        set((state) => ({ serverUrl: url, apiToken: url === state.serverUrl ? state.apiToken : '' }));
      },

      setRssiScanEnabled: (value) => {
        set({ rssiScanEnabled: value });
      },

      setTheme: (theme) => {
        set({ theme });
      },

      setAlertSoundEnabled: (value) => {
        set({ alertSoundEnabled: value });
      },
    }),
    {
      name: 'wifi-densepose-settings',
      storage: createJSONStorage(() => AsyncStorage),
      // Credentials are session-only, never written to unencrypted AsyncStorage.
      partialize: ({ apiToken: _token, ...settings }) => settings,
    },
  ),
);
