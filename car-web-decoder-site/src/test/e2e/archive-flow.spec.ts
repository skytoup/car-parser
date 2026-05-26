import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

const fixturePath = fileURLToPath(
  new URL("../../../../car-tests/data/Assets.car", import.meta.url),
);

test.skip(!existsSync(fixturePath), `fixture not found: ${fixturePath}`);
test.setTimeout(120_000);

test("加载本地 Assets.car、选择资源并触发批量下载", async ({ page }) => {
  await page.goto("/");

  await page.locator('input[type="file"]').setInputFiles(fixturePath);

  await expect(page.getByText("已完成")).toBeVisible({ timeout: 120_000 });

  const resourceButtons = page.locator("li > button[type='button']");
  await expect(resourceButtons.first()).toBeVisible({ timeout: 120_000 });
  await resourceButtons.first().click();

  await expect(page.getByText("当前条目 ID")).toBeVisible();

  const downloadPromise = page.waitForEvent("download", { timeout: 120_000 });
  await page.getByRole("button", { name: "下载全部 (ZIP)" }).click();
  const download = await downloadPromise;

  expect(download.suggestedFilename()).toMatch(/\.zip$/);
});

test("加载本地 Assets.car 后可预览 png_p3 的 argb16 Deepmap2 条目", async ({ page }) => {
  await page.goto("/");

  await page.locator('input[type="file"]').setInputFiles(fixturePath);
  await expect(page.getByText("已完成")).toBeVisible({ timeout: 120_000 });

  await page.getByPlaceholder("搜索资源 ID、Facet、Rendition...").fill("png_p3");

  const argb16Card = page.locator("li > button[type='button']").filter({ hasText: "argb16" });
  await expect(argb16Card).toHaveCount(1, { timeout: 120_000 });
  await argb16Card.click();

  await expect(page.getByText("当前条目 ID")).toBeVisible();
  await expect(page.getByText("entry-19", { exact: true })).toBeVisible();

  const detailPreview = page.getByTestId("detail-preview-canvas");
  await expect(detailPreview.locator("canvas")).toBeVisible({ timeout: 120_000 });
  await expect(page.getByText(/^预览失败:/)).toHaveCount(0);
});
