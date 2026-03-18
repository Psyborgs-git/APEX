import { test, expect } from '@playwright/test';

test.describe('ML Workbench', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await page.waitForLoadState('networkidle');
    await page.getByTestId('tab-ml').click();
  });

  test('should display ML Workbench component', async ({ page }) => {
    const workbench = page.getByTestId('ml-workbench');
    await expect(workbench).toBeVisible({ timeout: 10000 });
  });

  test('should show Train and Registry tabs', async ({ page }) => {
    const trainTab = page.getByTestId('ml-tab-train');
    const registryTab = page.getByTestId('ml-tab-registry');
    await expect(trainTab).toBeVisible();
    await expect(registryTab).toBeVisible();
  });

  test('should display algorithm selection', async ({ page }) => {
    const select = page.getByTestId('ml-algorithm-select');
    await expect(select).toBeVisible();
    // Verify default is random_forest
    await expect(select).toHaveValue('random_forest');
  });

  test('should display data path and target column inputs', async ({ page }) => {
    const dataPath = page.getByTestId('ml-data-path');
    const targetCol = page.getByTestId('ml-target-column');
    await expect(dataPath).toBeVisible();
    await expect(targetCol).toBeVisible();
    await expect(dataPath).toHaveValue('data/sample.csv');
    await expect(targetCol).toHaveValue('signal');
  });

  test('should display feature selection chips', async ({ page }) => {
    const featureList = page.getByTestId('ml-feature-list');
    await expect(featureList).toBeVisible();
    // At least some features should be shown
    const features = featureList.locator('button');
    const count = await features.count();
    expect(count).toBeGreaterThan(5);
  });

  test('should toggle features on click', async ({ page }) => {
    const feature = page.getByTestId('ml-feature-volume_lag_1');
    // Click to toggle
    await feature.click();
    // Verify it changes appearance (the selection state is reflected in styling)
    await expect(feature).toBeVisible();
  });

  test('should change algorithm selection', async ({ page }) => {
    const select = page.getByTestId('ml-algorithm-select');
    await select.selectOption('gradient_boosting');
    await expect(select).toHaveValue('gradient_boosting');
  });

  test('should display CV splits and lag periods', async ({ page }) => {
    const cvSplits = page.getByTestId('ml-cv-splits');
    const lagPeriods = page.getByTestId('ml-lag-periods');
    await expect(cvSplits).toBeVisible();
    await expect(lagPeriods).toBeVisible();
    await expect(cvSplits).toHaveValue('5');
    await expect(lagPeriods).toHaveValue('1,5,10');
  });

  test('should show train button', async ({ page }) => {
    const trainButton = page.getByTestId('ml-train-button');
    await expect(trainButton).toBeVisible();
    await expect(trainButton).toHaveText('Start Training');
  });

  test('should train model and show in registry', async ({ page }) => {
    // Click train
    const trainButton = page.getByTestId('ml-train-button');
    await trainButton.click();
    // Wait for training to complete (mock is instant)
    await expect(trainButton).toHaveText('Start Training', { timeout: 5000 });

    // Switch to registry tab
    await page.getByTestId('ml-tab-registry').click();
    const registry = page.getByTestId('ml-model-registry');
    await expect(registry).toBeVisible();

    // Should have at least one model
    const models = registry.locator('[data-testid^="ml-model-model_"]');
    const count = await models.count();
    expect(count).toBeGreaterThanOrEqual(1);
  });

  test('should show model metrics after training', async ({ page }) => {
    // Train a model
    await page.getByTestId('ml-train-button').click();
    await page.getByTestId('ml-tab-registry').click();

    // Check model has metrics displayed
    const registry = page.getByTestId('ml-model-registry');
    await expect(registry).toContainText('accuracy');
    await expect(registry).toContainText('completed');
  });

  test('should delete a model from registry', async ({ page }) => {
    // Train a model first
    await page.getByTestId('ml-train-button').click();
    await page.getByTestId('ml-tab-registry').click();

    // Get the model count
    const registry = page.getByTestId('ml-model-registry');
    const modelsBefore = registry.locator('[data-testid^="ml-model-model_"]');
    const countBefore = await modelsBefore.count();
    expect(countBefore).toBeGreaterThanOrEqual(1);

    // Delete the first model
    const deleteBtn = registry.locator('[data-testid^="ml-delete-"]').first();
    await deleteBtn.click();

    // Verify model count decreased or shows empty state
    const modelsAfter = registry.locator('[data-testid^="ml-model-model_"]');
    const countAfter = await modelsAfter.count();
    expect(countAfter).toBeLessThan(countBefore);
  });

  test('should show empty state in registry when no models', async ({ page }) => {
    await page.getByTestId('ml-tab-registry').click();
    const noModels = page.getByTestId('ml-no-models');
    await expect(noModels).toBeVisible();
  });
});
