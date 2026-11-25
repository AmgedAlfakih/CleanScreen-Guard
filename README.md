<div align="center">

# حارس الشاشة النظيفة — CleanScreen Guard

**تطبيق مجاني ومحلي بالكامل لنظام ويندوز 11، مكتوب بلغة Rust الأصلية، لاكتشاف وتمويه المحتوى غير اللائق في الوقت الفعلي عبر الألعاب والفيديوهات والمتصفحات.**

**A free, 100% local, GPU-accelerated Windows 11 desktop app written in native Rust that detects and blurs NSFW and suggestive content in real time across games, videos, and browsers.**

[العربية](#arabic) • [English](#english)

</div>

---

<a id="arabic"></a>
<div dir="rtl">

# حارس الشاشة النظيفة (CleanScreen Guard)

**حارس الشاشة النظيفة (CleanScreen Guard)** هو تطبيق مجاني ومفتوح المصدر لنظام **ويندوز 11**، يعمل محلياً بنسبة **100%** ومعزّز بتسريع كرت الشاشة (GPU)، ومكتوب بالكامل بلغة **Rust الأصلية** لاكتشاف وتمويه المحتوى غير اللائق والمثير في الوقت الفعلي عبر **الألعاب، والفيديوهات، والمتصفحات**.

يعمل كل شيء دون اتصال بالإنترنت على جهازك مباشرة — بلا رفع سحابي، بلا جمع بيانات أو تتبع، وبلا عبء مكتبات Python أو Qt أو OpenCV الثقيلة.

---

## المميزات الرئيسية

- **محرك Rust أصلي (`v2.1`):** استهلاك منخفض جداً للذاكرة العشوائية (~40–60 ميجابايت)، دون تقطّع أو توقّف مفاجئ، مع التقاط شاشة مباشر عبر **Windows DXGI Desktop Duplication** وطبقة تمويه شفافة تسمح بمرور النقرات (تستخدم خاصية `WDA_EXCLUDEFROMCAPTURE` لمنع حلقات التمويه المتكررة).
- **الفحص المتدرج الذكي على مرحلتين (`320n_fp16` ← `640m_fp16`):** يعمل النموذج الخفيف `320n_fp16` ككشّاف سريع على الشاشات النظيفة بأقل استهلاك ممكن لكرت الشاشة، ولا يستدعي النموذج الدقيق `640m_fp16` إلا عند الاشتباه بوجود محتوى أو أثناء فحوصات الأمان الدورية. كما يمكنك إضافة أي نموذج YOLO/NudeNet بصيغة `.onnx` بوضعه داخل مجلد `models/`.
- **تصغير الإطارات على كرت الشاشة (D3D11) وربط DirectML (`IoBinding`):** يستخدم التوليد العتادي للخرائط المصغّرة (`GenerateMips`) لتصغير الإطارات داخل كرت الشاشة مباشرة (مما يقلل نقل البيانات عبر منفذ PCIe بمقدار **4 إلى 16 ضعفاً** على الشاشات النظيفة)، ويتجاهل التحديثات التي تقتصر على حركة مؤشر الفأرة، ويعيد استخدام مخازن DirectML والذاكرة مسبقة الحجز لضمان استقرار تام في ذاكرة كرت الشاشة (VRAM) دون أي تسريب.
- **خيارات مرنة لتسريع العتاد:** يدعم وضع **كرت الشاشة المدمج الموفّر للطاقة** (الأفضل للابتوب ولإبقاء الكرت المنفصل عند 0% أثناء الألعاب)، والوضع **التلقائي**، ووضع **كرت الشاشة المنفصل (RTX/RX)**، ووضع **المعالج فقط (CPU Only)** باستهلاك 0% لكرت الشاشة.
- **تتبع حركة سلس ووضع سكون حقيقي:** خيط ذكاء اصطناعي منفصل غير متزامن مقترن بمحرك تتبع حركة مكتوب بلغة Rust الخالصة (حتى 30 إطاراً في الثانية)، ومعدل فحص تكيّفي، وإعادة تدوير للإطارات دون تخصيص ذاكرة متكرر، وسكون تلقائي كامل عند ثبات الشاشة.
- **واجهة إعدادات ثنائية اللغة (العربية RTL والإنجليزية):** مبنية على **Edge WebView2** المدمج في ويندوز 11 بالوضع الداكن، مع تحكم فوري في قوة التمويه، وحساسية الاكتشاف، ومدة ثبات التمويه لمنع الوميض، وهامش التوسعة، ومعدل فحص الذكاء الاصطناعي، وسلاسة التتبع، وخيار الفحص المتدرج، وتخصيص الفئات المراد تمويهها.
- **التحكم من شريط المهام (System Tray):** نقرة يسار لفتح الإعدادات؛ نقرة يمين لإيقاف الحماية مؤقتاً أو استئنافها أو إغلاق البرنامج، مع خيار التشغيل التلقائي مع بدء ويندوز.

---

## اختبارات الأداء الواقعية (`v2.1.0`)

تم القياس على **لابتوب بنظام ويندوز 11 وكرت شاشة NVIDIA RTX 3050M (بقدرة 60 واط وذاكرة 4 جيجابايت VRAM)** عبر DirectML:

### سرعة الاستدلال للنماذج (DirectML)

| النموذج | الإصدار الأساسي FP32 | الإصدار المحسّن Static FP16 (`v2.1`) | نسبة التسريع |
| :--- | :---: | :---: | :---: |
| **`320n` (الكشّاف السريع)** | `5.65 ms/frame` | **`2.52 ms/frame`** | **أسرع بـ 2.24×** |
| **`640m` (المدقّق عالي الدقة)** | `31.23 ms/frame` | **`13.24 ms/frame`** | **أسرع بـ 2.36×** |

### الأثر الفعلي على الجهاز (سجل GPU-Z لمدة دقيقتين أثناء الفحص النشط بمعدل `4 Hz` وتتبع `24 FPS`)

| المؤشر | النتيجة المقاسة |
| :--- | :--- |
| **استهلاك طاقة شريحة GPU** | **0.93 واط بالمتوسط** (`0.2–2.9 W`) |
| **إجمالي طاقة بطاقة الشاشة** | **6.31 واط بالمتوسط** (`3.5–10.8 W`، نحو 10.5% من حد 60 واط) |
| **الجهد وتردد النواة** | ثابت عند حد الخمول **`0.631 V`** (`210–645 MHz`، حالة `PerfCap = Idle`) |
| **درجة حرارة كرت الشاشة** | **46.5 °C بالمتوسط** (`45.3–47.5 °C`) — مراوح اللابتوب صامتة تماماً |
| **استهلاك ذاكرة VRAM** | **386 ميجابايت** (ثابت تماماً طوال الاختبار، صفر تسريب ذاكرة) |
| **حمل ناقل PCIe** | **0–1%** (`≤ 0.04 GB/s` إرسال، `≤ 0.23 GB/s` استقبال) |
| **الحمل الفعلي المعادل على GPU** | **~4.0% بالمتوسط** أثناء التتبع النشط المستمر (`< 1%` على الشاشات النظيفة) |

---

## التحميل والتثبيت

1. انتقل إلى صفحة [**الإصدارات (Releases)**](https://github.com/AmgedAlfakih/CleanScreen-Guard/releases) وحمّل ملف `CleanScreenGuard_Setup_2.1.0.exe`.
2. شغّل المثبّت (لا يتطلب صلاحيات المسؤول).
3. افتح **CleanScreen Guard** — سيعمل بهدوء في شريط النظام (بجانب الساعة).

> **الأمان والموثوقية:** جميع المثبّتات الرسمية **مبنية سحابياً مباشرة من هذا المستودع العام** عبر GitHub Actions، مع نشر بصمة التحقق **SHA-256** في ملاحظات كل إصدار.

---

## البناء من الكود المصدري (ويندوز 11)

**المتطلبات:** أداة Rust المستقرة (`rustup`)، وحزمة أدوات بناء Visual Studio C++ مع Windows 11 SDK، واختيارياً Inno Setup 6/7 لبناء ملف التثبيت.

1. حمّل مكتبات Microsoft ONNX Runtime DirectML الرسمية داخل مجلد `runtime/`:

<div dir="ltr">

```powershell
powershell -ExecutionPolicy Bypass -File .\scriptsetch_runtime.ps1
```

</div>

2. ضع ملفات النماذج `320n.onnx` و`320n_fp16.onnx` و`640m_fp16.onnx` داخل مجلد `runtime/` (يمكنك توليد نماذج FP16 الثابتة من ملفات FP32 `.onnx` عبر `scripts/optimize_models.py`).
3. قم بالبناء والتشغيل بوضع الإصدار (Release):

<div dir="ltr">

```powershell
powershell -ExecutionPolicy Bypass -File .uild_and_run.ps1
```

</div>

4. *(اختياري)* لبناء مثبّت ويندوز عبر Inno Setup 6/7:

<div dir="ltr">

```powershell
ISCC.exe /DMyAppVersion=2.1.0 installer.iss
```

</div>

---

## الترخيص

البرنامج مفتوح المصدر بموجب رخصة **GNU Affero General Public License v3.0 (AGPL-3.0)**.

</div>

---

<a id="english"></a>

# CleanScreen Guard (English)

**CleanScreen Guard** is a free, 100% local, GPU-accelerated Windows 11 desktop app written in **native Rust** that detects and blurs NSFW and suggestive content in real time across **games, videos, and browsers**.

Everything runs completely offline on your machine—zero cloud uploads, zero telemetry, and no Python, Qt, or OpenCV runtime bloat.

---

## Features

- **Native Rust Engine (`v2.1`):** Low RAM footprint (~40–60 MB), zero garbage-collection pauses, native Windows DXGI Desktop Duplication capture, and per-pixel-alpha click-through overlays (`WDA_EXCLUDEFROMCAPTURE` prevents blur feedback loops).
- **Two-Stage AI Cascade (`320n_fp16` → `640m_fp16`):** Lightweight `320n_fp16` scouts clean frames at ultra-low GPU cost, waking high-accuracy `640m_fp16` only when a candidate is detected or during periodic safety sweeps. Drop any compatible YOLO/NudeNet `.onnx` model into the `models/` folder to add custom models.
- **D3D11 GPU-Side Downscaling & DirectML `IoBinding`:** Hardware `GenerateMips` downscales frames directly on the GPU (cutting clean-screen PCIe readback by **4×–16×**), skips cursor-only desktop updates, and reuses pre-bound DirectML tensors and frame buffers with zero VRAM leaks.
- **Flexible Hardware Acceleration:** Supports **Power Saving GPU** (Integrated GPU — keeps dedicated GPUs at 0% while gaming), **Auto**, **High Performance GPU** (Dedicated RTX/RX), and **CPU Only** (0% GPU usage).
- **Smooth Motion Tracking & True Idle Sleep:** Decoupled async AI thread paired with pure-Rust block-matching motion tracking (up to 30 FPS), Adaptive Scan Rate, zero-allocation frame recycling, and automatic idle sleep on still screens.
- **Bilingual Settings UI (Arabic RTL & English):** Native Windows 11 Edge WebView2 dark-mode UI with live sliders for blur strength, sensitivity, anti-flicker hold time, padding, AI scan rate, tracking FPS, Two-Stage Cascade toggle, and per-category NudeNet toggles.
- **System Tray Controls:** Left-click the tray icon for Settings; right-click to Pause/Resume Protection or Quit, with optional Windows startup integration.

---

## Real-World Performance Benchmarks (`v2.1.0`)

Measured on a **Windows 11 laptop with an NVIDIA RTX 3050M (60W, 4 GB VRAM)** using DirectML:

### Model Inference Speed (DirectML)

| Model | FP32 Baseline | Static FP16 (`v2.1`) | Speedup |
| :--- | :---: | :---: | :---: |
| **`320n` (Scout)** | `5.65 ms/frame` | **`2.52 ms/frame`** | **2.24× faster** |
| **`640m` (Verifier)** | `31.23 ms/frame` | **`13.24 ms/frame`** | **2.36× faster** |

### Live System Impact (2-Minute GPU-Z Log, Active Detection at `4 Hz` + `24 FPS` Tracking)

| Metric | Measured Result |
| :--- | :--- |
| **GPU Chip Power Draw** | **0.93 W avg** (`0.2–2.9 W`) |
| **Total GPU Board Power** | **6.31 W avg** (`3.5–10.8 W`, ~10.5% of 60W TDP) |
| **GPU Voltage / Clocks** | Locked at **`0.631 V`** idle floor (`210–645 MHz`, `PerfCap = Idle`) |
| **GPU Temperature** | **46.5 °C avg** (`45.3–47.5 °C`) — silent laptop fans |
| **VRAM Footprint** | **386 MB** (completely flat across entire run, zero leaks) |
| **PCIe Bus Load** | **0–1%** (`≤ 0.04 GB/s` TX, `≤ 0.23 GB/s` RX) |
| **Clock-Normalized GPU Load** | **~4.0% avg** under continuous active tracking (`< 1%` on clean screens) |

---

## Download & Installation

1. Go to the [**Releases**](https://github.com/AmgedAlfakih/CleanScreen-Guard/releases) page and download `CleanScreenGuard_Setup_2.1.0.exe`.
2. Run the installer (no administrator rights required).
3. Launch **CleanScreen Guard**—it runs quietly in the Windows system tray.

> **Security & Trust:** All official installers are **cloud-built directly from this public repository** via GitHub Actions, with SHA-256 checksums published in each release's notes.

---

## Building from Source (Windows 11)

**Prerequisites:** Rust stable toolchain (`rustup`), Visual Studio C++ Build Tools + Windows 11 SDK, and optionally Inno Setup 6/7 for packaging.

1. Download the official Microsoft ONNX Runtime DirectML DLLs into `runtime/`:
   ```powershell
   powershell -ExecutionPolicy Bypass -File .\scriptsetch_runtime.ps1
   ```
2. Place `320n.onnx`, `320n_fp16.onnx`, and `640m_fp16.onnx` inside `runtime/` (you can generate static FP16 models from FP32 `.onnx` files using `scripts/optimize_models.py`).
3. Build and launch in Release mode:
   ```powershell
   powershell -ExecutionPolicy Bypass -File .uild_and_run.ps1
   ```
4. *(Optional)* Compile the Windows installer with Inno Setup 6/7:
   ```powershell
   ISCC.exe /DMyAppVersion=2.1.0 installer.iss
   ```

---

## License

Open-source under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.
