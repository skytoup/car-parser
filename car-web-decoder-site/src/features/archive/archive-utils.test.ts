import { dedupeFileName, deriveZipName, sanitizeFileName, toUint8Array } from "@/features/archive/archive-utils";

describe("archive-utils", () => {
  it("dedupeFileName 会在重名时追加序号，并保留扩展名", () => {
    const names = new Set<string>();

    expect(dedupeFileName(" icon?.png ", names)).toBe("icon_.png");
    expect(dedupeFileName(" icon?.png ", names)).toBe("icon_ (2).png");
    expect(dedupeFileName(" icon?.png ", names)).toBe("icon_ (3).png");
  });

  it("dedupeFileName 在无扩展名时同样去重", () => {
    const names = new Set<string>();

    expect(dedupeFileName("asset", names)).toBe("asset");
    expect(dedupeFileName("asset", names)).toBe("asset (2)");
  });

  it("deriveZipName 按来源文件名推导 zip 名称", () => {
    expect(deriveZipName(null)).toBe("car-assets.zip");
    expect(deriveZipName("   ")).toBe("car-assets.zip");
    expect(deriveZipName(" Theme/Assets.car ")).toBe("Theme_Assets-assets.zip");
    expect(deriveZipName("Assets")).toBe("Assets-assets.zip");
  });

  it("toUint8Array 在数组输入时转换，在 Uint8Array 输入时直接返回", () => {
    const raw = [1, 2, 3];
    const converted = toUint8Array(raw);
    expect(converted).toBeInstanceOf(Uint8Array);
    expect(Array.from(converted)).toEqual(raw);

    const bytes = new Uint8Array([4, 5, 6]);
    expect(toUint8Array(bytes)).toBe(bytes);
  });

  it("sanitizeFileName 在非法字符和空值场景下产出可写文件名", () => {
    expect(sanitizeFileName("a/b:c*?.png")).toBe("a_b_c_.png");
    expect(sanitizeFileName("   ")).toBe("asset.bin");
  });
});
