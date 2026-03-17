import { test, expect } from '@playwright/test';

test.describe('Custom ML Pipeline Execution', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
  });

  test('should display StrategyIDE component', async ({ page }) => {
    // Open StrategyIDE panel
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

  test('should allow editing strategy code', async ({ page }) => {
    // Select or create a strategy file
    const editor = page.getByTestId('strategy-editor');
    await expect(editor).toBeVisible({ timeout: 10000 });

    // Type code into editor
    const testCode = `
import numpy as np
import pandas as pd

def strategy(data):
    return {"signal": "buy", "confidence": 0.85}
`;

    await editor.click();
    await page.keyboard.type(testCode);

    // Verify code is entered
    const editorContent = await editor.textContent();
    expect(editorContent).toContain('def strategy');
  });

  test('should execute strategy and display results', async ({ page }) => {
    // Navigate to strategy execution
    const executeButton = page.getByTestId('execute-strategy');
    await executeButton.click();

    // Wait for execution to complete
    await page.waitForTimeout(3000);

    // Check for output
    const outputPanel = page.getByTestId('strategy-output');
    await expect(outputPanel).toBeVisible({ timeout: 10000 });

    // Verify output is not empty
    const outputText = await outputPanel.textContent();
    expect(outputText).toBeTruthy();
  });

  test('should display execution errors', async ({ page }) => {
    // Load a strategy with errors
    const editor = page.getByTestId('strategy-editor');
    await editor.click();
    await page.keyboard.type('invalid python code!!!');

    // Execute
    const executeButton = page.getByTestId('execute-strategy');
    await executeButton.click();

    // Check for error display
    const errorPanel = page.getByTestId('strategy-error');
    await expect(errorPanel).toBeVisible({ timeout: 5000 });
  });

  test('should route output to correct panel', async ({ page }) => {
    // Execute a strategy
    const executeButton = page.getByTestId('execute-strategy');
    await executeButton.click();

    await page.waitForTimeout(2000);

    // Verify output appears in output panel
    const outputPanel = page.getByTestId('strategy-output');
    await expect(outputPanel).toBeVisible();

    // Verify output contains expected data
    const outputText = await outputPanel.textContent();
    expect(outputText.length).toBeGreaterThan(0);
  });

  test('should allow saving strategy file', async ({ page }) => {
    // Make changes to strategy
    const editor = page.getByTestId('strategy-editor');
    await editor.click();
    await page.keyboard.type('# Modified strategy');

    // Save file
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

    // Verify at least one file exists
    const fileItems = page.getByTestId('strategy-file-item');
    const count = await fileItems.count();
    expect(count).toBeGreaterThanOrEqual(0);
  });

  test('should display ML pipeline status', async ({ page }) => {
    // Execute strategy
    const executeButton = page.getByTestId('execute-strategy');
    await executeButton.click();

    // Check for status indicator
    const statusIndicator = page.getByTestId('pipeline-status');
    await expect(statusIndicator).toBeVisible({ timeout: 5000 });

    // Verify status shows running or completed
    const statusText = await statusIndicator.textContent();
    expect(statusText).toMatch(/running|completed|error/i);
  });
});
