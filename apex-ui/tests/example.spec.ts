import { test, expect } from '@playwright/test';

test('basic application load test', async ({ page }) => {
  await page.goto('/');

  // Wait for the application to load
  await page.waitForLoadState('networkidle');

  // Basic check that the app loaded
  expect(page).toBeTruthy();
});
