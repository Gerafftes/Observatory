// Internationalization - DE/EN/PL language support
// Detects browser language, persists choice, translates UI strings

const translations = {
  en: {
    // Navigation
    'nav.dashboard': 'Dashboard',
    'nav.hardware': 'Hardware',
    'nav.demo': 'Live Demo',
    'nav.sensing': 'Sensing',
    'nav.training': 'Training',
    'nav.more': 'More',
    'nav.moreTools': 'Tools',
    'nav.poseFusion': 'Pose Fusion',
    'nav.observatory': 'Observatory',

    // Dashboard
    'dashboard.title': 'WiFi Human Pose',
    'dashboard.subtitle': 'WiFi sensing through walls',
    'dashboard.description': 'Track movement through walls with WiFi—no camera needed.',
    'dashboard.status': 'System Status',
    'dashboard.liveStats': 'Live Statistics',
    'dashboard.activePersons': 'Active Persons',
    'dashboard.avgConfidence': 'Avg Confidence',
    'dashboard.totalDetections': 'Total Detections',

    // Status
    'status.apiServer': 'API Server',
    'status.hardware': 'Hardware',
    'status.inference': 'Inference',
    'status.streaming': 'Streaming',
    'status.dataSource': 'Data Source',

    // Actions
    'action.startDetection': 'Start Detection',
    'action.stopDetection': 'Stop Detection',
    'action.toggleTheme': 'Toggle theme',
    'action.exportData': 'Export data',
    'action.screenshot': 'Take screenshot',

    // Connection
    'conn.connected': 'Connected',
    'conn.connecting': 'Connecting...',
    'conn.offline': 'Offline',
    'conn.reconnecting': 'Reconnecting...',
    'conn.live': 'Live',
    'conn.simulated': 'Simulated',

    // Misc
    'misc.loading': 'Loading...',
    'misc.error': 'An error occurred',
    'misc.noData': 'No data available',
    'misc.close': 'Close',
    'misc.cancel': 'Cancel',
    'misc.confirm': 'Confirm',
    'misc.settings': 'Settings',
    'misc.language': 'Language',

    // Quick settings
    'settings.title': 'Settings',
    'settings.display': 'Display',
    'settings.reducedMotion': 'Reduced motion',
    'settings.highContrast': 'High contrast',
    'settings.compactMode': 'Compact mode',
    'settings.monitoring': 'Monitoring',
    'settings.healthPolling': 'Health polling',
    'settings.autoReconnect': 'Auto-reconnect',
    'settings.data': 'Data',
    'settings.clearLocalData': 'Clear local data',
    'settings.resetOnboarding': 'Reset onboarding',
    'settings.clear': 'Clear',
    'settings.reset': 'Reset'
  },

  de: {
    // Navigation
    'nav.dashboard': 'Dashboard',
    'nav.hardware': 'Hardware',
    'nav.demo': 'Live-Demo',
    'nav.sensing': 'Sensing',
    'nav.training': 'Training',
    'nav.more': 'Mehr',
    'nav.moreTools': 'Werkzeuge',
    'nav.poseFusion': 'Pose Fusion',
    'nav.observatory': 'Observatory',

    // Dashboard
    'dashboard.title': 'WiFi-Pose erkennen',
    'dashboard.subtitle': 'WiFi-Sensorik durch Wände',
    'dashboard.description': 'Bewegung per WiFi erfassen – ohne Kamera.',
    'dashboard.status': 'Systemstatus',
    'dashboard.liveStats': 'Live-Statistiken',
    'dashboard.activePersons': 'Aktive Personen',
    'dashboard.avgConfidence': 'Durchschnittliche Konfidenz',
    'dashboard.totalDetections': 'Detektionen gesamt',

    // Status
    'status.apiServer': 'API-Server',
    'status.hardware': 'Hardware',
    'status.inference': 'Inferenz',
    'status.streaming': 'Streaming',
    'status.dataSource': 'Datenquelle',

    // Actions
    'action.startDetection': 'Detektion starten',
    'action.stopDetection': 'Detektion stoppen',
    'action.toggleTheme': 'Darstellung wechseln',
    'action.exportData': 'Daten exportieren',
    'action.screenshot': 'Screenshot aufnehmen',

    // Connection
    'conn.connected': 'Verbunden',
    'conn.connecting': 'Verbinde …',
    'conn.offline': 'Offline',
    'conn.reconnecting': 'Verbinde erneut …',
    'conn.live': 'Live',
    'conn.simulated': 'Simuliert',

    // Misc
    'misc.loading': 'Lade …',
    'misc.error': 'Ein Fehler ist aufgetreten',
    'misc.noData': 'Keine Daten verfügbar',
    'misc.close': 'Schließen',
    'misc.cancel': 'Abbrechen',
    'misc.confirm': 'Bestätigen',
    'misc.settings': 'Einstellungen',
    'misc.language': 'Sprache',

    // Quick settings
    'settings.title': 'Einstellungen',
    'settings.display': 'Darstellung',
    'settings.reducedMotion': 'Weniger Bewegung',
    'settings.highContrast': 'Hoher Kontrast',
    'settings.compactMode': 'Kompakte Ansicht',
    'settings.monitoring': 'Überwachung',
    'settings.healthPolling': 'Gesundheitsabfragen',
    'settings.autoReconnect': 'Automatisch verbinden',
    'settings.data': 'Daten',
    'settings.clearLocalData': 'Lokale Daten löschen',
    'settings.resetOnboarding': 'Einführung zurücksetzen',
    'settings.clear': 'Löschen',
    'settings.reset': 'Zurücksetzen'
  },

  pl: {
    // Navigation
    'nav.dashboard': 'Panel',
    'nav.hardware': 'Sprzet',
    'nav.demo': 'Demo na zywo',
    'nav.sensing': 'Czujniki',
    'nav.training': 'Trening',
    'nav.more': 'Wiecej',
    'nav.moreTools': 'Narzedzia',
    'nav.poseFusion': 'Pose Fusion',
    'nav.observatory': 'Observatory',

    // Dashboard
    'dashboard.title': 'Poza czlowieka przez WiFi',
    'dashboard.subtitle': 'Sensing WiFi przez sciany',
    'dashboard.description': 'Ruch przez sciany — bez kamery.',
    'dashboard.status': 'Status systemu',
    'dashboard.liveStats': 'Statystyki na zywo',
    'dashboard.activePersons': 'Aktywne osoby',
    'dashboard.avgConfidence': 'Srednia pewnosc',
    'dashboard.totalDetections': 'Laczne detekcje',

    // Status
    'status.apiServer': 'Serwer API',
    'status.hardware': 'Sprzet',
    'status.inference': 'Wnioskowanie',
    'status.streaming': 'Streaming',
    'status.dataSource': 'Zrodlo danych',

    // Actions
    'action.startDetection': 'Rozpocznij detekcje',
    'action.stopDetection': 'Zatrzymaj detekcje',
    'action.toggleTheme': 'Zmien motyw',
    'action.exportData': 'Eksportuj dane',
    'action.screenshot': 'Zrob zrzut ekranu',

    // Connection
    'conn.connected': 'Polaczono',
    'conn.connecting': 'Laczenie...',
    'conn.offline': 'Offline',
    'conn.reconnecting': 'Ponowne laczenie...',
    'conn.live': 'Na zywo',
    'conn.simulated': 'Symulacja',

    // Misc
    'misc.loading': 'Ladowanie...',
    'misc.error': 'Wystapil blad',
    'misc.noData': 'Brak danych',
    'misc.close': 'Zamknij',
    'misc.cancel': 'Anuluj',
    'misc.confirm': 'Potwierdz',
    'misc.settings': 'Ustawienia',
    'misc.language': 'Jezyk',

    // Quick settings
    'settings.title': 'Ustawienia',
    'settings.display': 'Widok',
    'settings.reducedMotion': 'Mniej animacji',
    'settings.highContrast': 'Wysoki kontrast',
    'settings.compactMode': 'Tryb kompaktowy',
    'settings.monitoring': 'Monitoring',
    'settings.healthPolling': 'Odpytywanie stanu',
    'settings.autoReconnect': 'Automatyczne laczenie',
    'settings.data': 'Dane',
    'settings.clearLocalData': 'Wyczysc dane lokalne',
    'settings.resetOnboarding': 'Resetuj wprowadzenie',
    'settings.clear': 'Wyczysc',
    'settings.reset': 'Resetuj'
  }
};

export class I18n {
  constructor() {
    this.locale = this.getSavedLocale() || this.detectLocale();
    this.listeners = [];
  }

  init() {
    document.documentElement.setAttribute('lang', this.locale);
    this.createSelector();
    this.applyTranslations();
  }

  detectLocale() {
    const lang = navigator.language?.toLowerCase() || 'en';
    if (lang.startsWith('de')) return 'de';
    if (lang.startsWith('pl')) return 'pl';
    return 'en';
  }

  getSavedLocale() {
    try { return localStorage.getItem('ruview-locale'); }
    catch { return null; }
  }

  saveLocale(locale) {
    try { localStorage.setItem('ruview-locale', locale); }
    catch { /* noop */ }
  }

  t(key) {
    const dict = translations[this.locale] || translations.en;
    return dict[key] || translations.en[key] || key;
  }

  setLocale(locale) {
    if (!translations[locale]) return;
    this.locale = locale;
    this.saveLocale(locale);
    document.documentElement.setAttribute('lang', locale);
    this.applyTranslations();
    this.listeners.forEach(cb => { try { cb(locale); } catch { /* noop */ } });
  }

  onLocaleChange(callback) {
    this.listeners.push(callback);
    return () => {
      const i = this.listeners.indexOf(callback);
      if (i > -1) this.listeners.splice(i, 1);
    };
  }

  applyTranslations() {
    // Translate elements with data-i18n attribute
    document.querySelectorAll('[data-i18n]').forEach(el => {
      const key = el.getAttribute('data-i18n');
      el.textContent = this.t(key);
    });

    // Translate placeholders
    document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
      const key = el.getAttribute('data-i18n-placeholder');
      el.placeholder = this.t(key);
    });

    // Translate aria-labels
    document.querySelectorAll('[data-i18n-aria]').forEach(el => {
      const key = el.getAttribute('data-i18n-aria');
      el.setAttribute('aria-label', this.t(key));
    });

    // Translate titles on compact controls
    document.querySelectorAll('[data-i18n-title]').forEach(el => {
      const key = el.getAttribute('data-i18n-title');
      el.setAttribute('title', this.t(key));
    });

    // Update language selector
    const selector = document.getElementById('lang-selector');
    if (selector) selector.value = this.locale;
  }

  createSelector() {
    const wrapper = document.createElement('div');
    wrapper.className = 'lang-selector-wrap';
    wrapper.innerHTML = `
      <span class="lang-selector-label" data-i18n="misc.language">Language</span>
      <select id="lang-selector" class="lang-selector" aria-label="Language" data-i18n-aria="misc.language">
        <option value="de">DE</option>
        <option value="en">EN</option>
        <option value="pl">PL</option>
      </select>
    `;

    const select = wrapper.querySelector('select');
    select.value = this.locale;
    select.addEventListener('change', () => this.setLocale(select.value));

    const headerActions = document.querySelector('[data-header-actions]') || document.querySelector('.header-info');
    if (headerActions) {
      headerActions.appendChild(wrapper);
    }
  }

  getAvailableLocales() {
    return Object.keys(translations);
  }

  dispose() {
    this.listeners = [];
  }
}

export const i18n = new I18n();
