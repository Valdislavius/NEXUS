import { defineConfig } from '@playwright/test';
const externalBaseURL = process.env.E2E_BASE_URL;
const reportSuffix = externalBaseURL ? '-docker' : '';
export default defineConfig({
  testDir: './tests/e2e', fullyParallel: false, workers: 1,
  outputDir: `test-results${reportSuffix}`,
  timeout: 45000,
  reporter: [['list'], ['html', { open: 'never', outputFolder: `playwright-report${reportSuffix}` }], ['json', { outputFile: `test-results${reportSuffix}/results.json` }]],
  use: { baseURL: externalBaseURL ?? 'http://127.0.0.1:3001', channel: 'msedge', headless: true, trace: 'retain-on-failure', screenshot: 'only-on-failure', locale: 'ru-RU', reducedMotion: 'reduce' },
  webServer: externalBaseURL ? undefined : { command: 'npm run start -- --port 3001', url: 'http://127.0.0.1:3001', reuseExistingServer: false, timeout: 30000 },
});
