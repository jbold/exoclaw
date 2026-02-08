import { expect, test } from "@playwright/test";

const token = process.env.EXOCLAW_E2E_TOKEN ?? "e2e-test-token";

test("prompts for auth and reconnects with a token", async ({ page }) => {
  await page.goto("/");

  await expect(page.getByTestId("auth-overlay")).toBeVisible();
  await page.getByTestId("auth-token-input").fill(token);
  await page.getByTestId("auth-connect-button").click();
  await expect(page.getByTestId("auth-overlay")).toBeHidden();

  await page.getByTestId("connection-status").click();
  await expect(page.getByText("Connected")).toBeVisible();
});

