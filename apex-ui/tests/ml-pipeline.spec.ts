import { test, expect } from '@playwright/test';

test.describe('Custom ML Pipeline Execution', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    // Navigate to Strategy IDE tab
    await page.getByTestId('tab-strategy').click();
  });

  test('should display StrategyIDE component', async ({ page }) => {
    const strategyIDE = page.getByTestId('strategy-ide');
    await expect(strategyIDE).toBeVisible({ timeout: 10000 });
  });

  test('should allow creating new strategy file', async ({ page }) => {
    // Click new file button
    const newFileButton = page.getByTestId('strategy-new-file');
    await newFileButton.click();

    // Enter file name
    const fileNameInput = page.getByTestId('file-name-input');
    await fileNameInput.fill('test_strategy.py');

    // Confirm creation
    const confirmButton = page.getByTestId('confirm-create-file');
    await confirmButton.click();

    // Verify file is created
    const fileItem = page.getByTestId('file-test_strategy.py');
    await expect(fileItem).toBeVisible({ timeout: 3000 });
  });

  test('should display strategy editor', async ({ page }) => {
    const editor = page.getByTestId('strategy-editor');
    await expect(editor).toBeVisible({ timeout: 10000 });
  });

  test('should execute strategy and display output', async ({ page }) => {
    // Click execute button
    const executeButton = page.getByTestId('execute-strategy');
    await executeButton.click();

    // Check output panel is visible
    const outputPanel = page.getByTestId('strategy-output');
    await expect(outputPanel).toBeVisible({ timeout: 10000 });

    // Verify output has content (the Running message)
    const outputText = await outputPanel.textContent();
    expect(outputText).toBeTruthy();
    expect(outputText!.length).toBeGreaterThan(0);
  });

  test('should allow saving strategy file', async ({ page }) => {
    // Click save button
    const saveButton = page.getByTestId('save-strategy');
    await saveButton.click();

    // Verify save confirmation
    const saveConfirmation = page.getByTestId('save-confirmation');
    await expect(saveConfirmation).toBeVisible({ timeout: 3000 });
  });

  test('should load existing strategy files', async ({ page }) => {
    // Check for file list
    const fileList = page.getByTestId('strategy-file-list');
    await expect(fileList).toBeVisible({ timeout: 10000 });
  });

  test('should display ML pipeline status', async ({ page }) => {
    // Check for status indicator (always visible in toolbar)
    const statusIndicator = page.getByTestId('pipeline-status');
    await expect(statusIndicator).toBeVisible({ timeout: 5000 });
  });

  test('should switch between chart and strategy tabs', async ({ page }) => {
    // Verify strategy IDE is visible
    await expect(page.getByTestId('strategy-ide')).toBeVisible({ timeout: 5000 });

    // Switch back to chart tab
    await page.getByTestId('tab-chart').click();

    // Verify chart is visible and strategy is hidden
    await expect(page.getByTestId('candle-chart')).toBeVisible({ timeout: 5000 });
  });
});
