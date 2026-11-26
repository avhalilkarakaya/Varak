#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::hash_map::DefaultHasher;
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Mutex;

const PENCERESIZ: u32 = 0x0800_0000; // CREATE_NO_WINDOW
const PDF_BETIGI: &str = include_str!("pdf.js");
const UZANTILAR: &[&str] = &["pdf", "jpg", "jpeg", "png", "tif", "tiff", "bmp", "webp"];

// ---------- Yardımcılar ----------

fn hata(e: impl ToString) -> String {
    e.to_string()
}

fn araclar() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(hata)?;
    Ok(exe.parent().ok_or("Uygulama klasörü bulunamadı")?.join("araclar"))
}

fn gecici() -> PathBuf {
    std::env::temp_dir().join("varak")
}

fn klasor(yol: PathBuf) -> Result<PathBuf, String> {
    std::fs::create_dir_all(&yol).map_err(hata)?;
    Ok(yol)
}

fn pdf_mi(yol: &Path) -> bool {
    yol.extension().is_some_and(|u| u.eq_ignore_ascii_case("pdf"))
}

fn dosya_adi(yol: &Path) -> String {
    yol.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Dosya yolu, değişiklik zamanı ve ek bilgiden kısa bir anahtar üretir.
fn ozet(yol: &Path, ek: &str) -> String {
    let mut h = DefaultHasher::new();
    yol.hash(&mut h);
    std::fs::metadata(yol).and_then(|m| m.modified()).ok().hash(&mut h);
    ek.hash(&mut h);
    format!("{:x}", h.finish())
}

fn calistir(program: &Path, args: &[OsString]) -> Result<String, String> {
    let cikti = Command::new(program)
        .args(args)
        .creation_flags(PENCERESIZ)
        .output()
        .map_err(|e| format!("{} çalıştırılamadı: {e}", program.display()))?;
    if !cikti.status.success() {
        return Err(String::from_utf8_lossy(&cikti.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&cikti.stdout).into_owned())
}

fn mutool(args: &[OsString]) -> Result<String, String> {
    calistir(&araclar()?.join("mupdf").join("mutool.exe"), args)
}

fn betik(args: &[OsString]) -> Result<String, String> {
    let yol = klasor(gecici())?.join("pdf.js");
    std::fs::write(&yol, PDF_BETIGI).map_err(hata)?;
    let mut tum: Vec<OsString> = vec!["run".into(), yol.into()];
    tum.extend_from_slice(args);
    mutool(&tum)
}

fn onizleme_klasoru(yol: &Path) -> Result<PathBuf, String> {
    klasor(gecici().join("onizleme").join(ozet(yol, "")))
}

/// TIFF'teki gerçek sayfaların sıra numaraları (1'den başlar). Boyutu olmayan dizinler
/// (ör. sonradan eklenmiş üst veri) ve küçük önizleme kopyaları atlanır. Çözülemezse None.
fn tiff_sayfalari(d: &[u8]) -> Option<Vec<usize>> {
    let le = match d.get(..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let s16 = |i: usize| d.get(i..i + 2).map(|b| if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) });
    let s32 = |i: usize| {
        d.get(i..i + 4).map(|b| {
            let a = [b[0], b[1], b[2], b[3]];
            if le { u32::from_le_bytes(a) } else { u32::from_be_bytes(a) }
        })
    };
    let (mut sayfalar, mut ofs, mut no) = (Vec::new(), s32(4)? as usize, 0);
    while ofs != 0 && no < 100_000 {
        no += 1;
        let adet = s16(ofs)? as usize;
        let (mut gen, mut yuk, mut kucuk) = (0, 0, false);
        for k in 0..adet {
            let e = ofs + 2 + 12 * k;
            let deger = if s16(e + 2)? == 3 { s16(e + 8)? as u32 } else { s32(e + 8)? };
            match s16(e)? {
                256 => gen = deger,
                257 => yuk = deger,
                254 => kucuk = deger & 1 == 1,
                _ => {}
            }
        }
        if gen > 0 && yuk > 0 && !kucuk {
            sayfalar.push(no);
        }
        ofs = s32(ofs + 2 + 12 * adet)? as usize;
    }
    Some(sayfalar)
}

/// Girdinin işlenecek hâli ve görsel olup olmadığı: MuPDF'in okuyamadığı WebP PNG'ye,
/// TIFF (çok sayfalı olabilir) görüntüye dokunmadan PDF'e çevrilir; böylece TIFF her yerde PDF gibi işlenir.
/// Biçim dosya adından değil içerikten anlaşılır (.jpeg adlı WebP dosyaları da olabilir).
fn girdi(yol: &Path) -> Result<(PathBuf, bool), String> {
    if pdf_mi(yol) {
        return Ok((yol.to_path_buf(), false));
    }
    let okuyucu = image::ImageReader::open(yol).map_err(hata)?.with_guessed_format().map_err(hata)?;
    match okuyucu.format() {
        Some(image::ImageFormat::WebP) => {
            let png = onizleme_klasoru(yol)?.join("girdi.png");
            okuyucu.decode().map_err(hata)?.save(&png).map_err(hata)?;
            Ok((png, true))
        }
        Some(image::ImageFormat::Tiff) => {
            let pdf = onizleme_klasoru(yol)?.join("girdi.pdf");
            if !pdf.exists() {
                let yari = pdf.with_extension("yari.pdf");
                let mut args: Vec<OsString> = vec!["convert".into(), "-o".into(), yari.clone().into(), yol.into()];
                if let Some(s) = std::fs::read(yol).ok().as_deref().and_then(tiff_sayfalari).filter(|s| !s.is_empty()) {
                    args.push(s.iter().map(usize::to_string).collect::<Vec<_>>().join(",").into());
                }
                mutool(&args)?;
                std::fs::rename(yari, &pdf).map_err(hata)?;
            }
            Ok((pdf, false))
        }
        _ => Ok((yol.to_path_buf(), true)),
    }
}

fn kaydetme_penceresi(ad: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(ad).save_file()
}

/// Görünmez metin için Arapça harfleri de içeren Windows yazı tipi
fn font_yolu() -> PathBuf {
    PathBuf::from(std::env::var_os("WINDIR").unwrap_or("C:\\Windows".into()))
        .join("Fonts")
        .join("arial.ttf")
}

// ---------- PDF İşlemleri ----------

#[derive(serde::Serialize)]
struct Dosya {
    yol: String,
    ad: String,
    sayfa: usize,
    onizleme: String,
    resim: bool,
}

#[tauri::command]
async fn dosya_sec() -> Vec<String> {
    rfd::FileDialog::new()
        .add_filter("PDF ve görseller", UZANTILAR)
        .pick_files()
        .unwrap_or_default()
        .iter()
        .map(|p| p.display().to_string())
        .collect()
}

#[tauri::command]
async fn dosya_bilgi(yollar: Vec<String>) -> Result<Vec<Dosya>, String> {
    yollar
        .iter()
        .map(|y| -> Result<Dosya, String> {
            let yol = PathBuf::from(y);
            let (kaynak, resim) = girdi(&yol)?;
            let sayfa = if resim {
                1
            } else {
                betik(&["bilgi".into(), kaynak.clone().into()])?
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| format!("{} okunamadı", yol.display()))?
            };
            Ok(Dosya {
                yol: kaynak.display().to_string(),
                ad: dosya_adi(&yol),
                sayfa,
                onizleme: onizleme_klasoru(&yol)?.display().to_string(),
                resim,
            })
        })
        .collect()
}

#[tauri::command]
async fn onizleme(yol: String, klasor: String, ilk: usize, son: usize) -> Result<(), String> {
    let cikti = Path::new(&klasor).join("s%d.png");
    mutool(&[
        "draw".into(),
        "-w".into(),
        "160".into(),
        "-o".into(),
        cikti.into(),
        yol.into(),
        format!("{ilk}-{son}").into(),
    ])
    .map(|_| ())
}

/// Kırpma penceresi için tek sayfanın büyük önizlemesi
#[tauri::command]
async fn buyuk_onizleme(yol: String, klasor: String, no: usize) -> Result<String, String> {
    let cikti = Path::new(&klasor).join(format!("b{no}.png"));
    if !cikti.exists() {
        mutool(&["draw".into(), "-w".into(), "700".into(), "-o".into(), cikti.clone().into(), yol.into(), no.to_string().into()])?;
    }
    Ok(cikti.display().to_string())
}

#[tauri::command]
async fn kayit_yeri(oneri: String) -> Option<String> {
    kaydetme_penceresi(&oneri).map(|p| p.display().to_string())
}

#[tauri::command]
async fn klasor_sec() -> Option<String> {
    rfd::FileDialog::new().pick_folder().map(|p| p.display().to_string())
}

/// Listeye eklendikten sonra silinen ya da taşınan kaynak dosyaları önceden yakalar
fn kaynak_denetle(v: &serde_json::Value) -> Result<(), String> {
    match v {
        serde_json::Value::Array(a) => a.iter().try_for_each(kaynak_denetle),
        serde_json::Value::Object(o) => {
            for (anahtar, deger) in o {
                if let (true, Some(y)) = (anahtar == "kaynak" || anahtar == "resim", deger.as_str()) {
                    if !Path::new(y).exists() {
                        return Err(format!(
                            "Kaynak dosya bulunamadı: {y}. Dosya taşınmış, silinmiş ya da adı \
                             değiştirilmiş olabilir; bu dosyanın sayfalarını listeden çıkarıp yeniden ekleyin."
                        ));
                    }
                }
                kaynak_denetle(deger)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// PDF İşlemleri listesindeki sayfaları mevcut metindeki numaralara göre sıralar; raporu JSON döndürür
#[tauri::command]
async fn numara_sirala(plan: String) -> Result<String, String> {
    kaynak_denetle(&serde_json::from_str::<serde_json::Value>(&plan).map_err(hata)?)?;
    let yol = klasor(gecici())?.join("numara.json");
    std::fs::write(&yol, plan).map_err(hata)?;
    betik(&["numaralar".into(), yol.into()])
}

#[tauri::command]
async fn uygula(plan: String) -> Result<(), String> {
    kaynak_denetle(&serde_json::from_str::<serde_json::Value>(&plan).map_err(hata)?)?;
    let yol = klasor(gecici())?.join("plan.json");
    std::fs::write(&yol, plan).map_err(hata)?;
    betik(&["uygula".into(), yol.into()]).map(|_| ())
}

#[tauri::command]
async fn sikistir(seviye: String) -> Result<String, String> {
    if !["screen", "ebook", "printer"].contains(&seviye.as_str()) {
        return Err("Geçersiz sıkıştırma seviyesi".into());
    }
    let girdi = rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .pick_file()
        .ok_or("Dosya seçilmedi")?;
    let ad = format!("{}_sikistirilmis.pdf", girdi.file_stem().unwrap_or_default().to_string_lossy());
    let cikti = kaydetme_penceresi(&ad).ok_or("Kayıt yeri seçilmedi")?;
    // Ghostscript okuduğu dosyanın üzerine yazarsa kaynak bozulur
    if cikti.exists() && std::fs::canonicalize(&cikti).ok() == std::fs::canonicalize(&girdi).ok() {
        return Err("Kaynak dosyanın üzerine yazılamaz. Başka bir ad seçin.".into());
    }
    calistir(
        &araclar()?.join("gs").join("bin").join("gswin64c.exe"),
        &[
            "-sDEVICE=pdfwrite".into(),
            format!("-dPDFSETTINGS=/{seviye}").into(),
            "-dNOPAUSE".into(),
            "-dBATCH".into(),
            "-dQUIET".into(),
            format!("-sOutputFile={}", cikti.display()).into(),
            girdi.clone().into(),
        ],
    )?;
    let mb = |p: &Path| std::fs::metadata(p).map(|m| m.len() as f64 / 1_048_576.0).unwrap_or(0.0);
    Ok(format!("Sıkıştırıldı: {:.1} MB → {:.1} MB", mb(&girdi), mb(&cikti)))
}

// ---------- OCR ----------

#[derive(Clone, serde::Serialize)]
struct Durum {
    calisiyor: bool,
    dosya: String,
    dosya_no: usize,
    dosya_sayisi: usize,
    bitti: usize,
    toplam: usize,
    sonuclar: Vec<Sonuc>,
    #[serde(skip)]
    son: Option<(PathBuf, usize)>, // son okunan sayfa: iş klasörü, sayfa dizini
}

#[derive(Clone, serde::Serialize)]
struct Sonuc {
    mesaj: String,
    dosya: Option<String>,
    rapor: Option<serde_json::Value>,
}

impl Durum {
    const BOS: Durum = Durum {
        calisiyor: false,
        dosya: String::new(),
        dosya_no: 0,
        dosya_sayisi: 0,
        bitti: 0,
        toplam: 0,
        sonuclar: Vec::new(),
        son: None,
    };
}

static DURUM: Mutex<Durum> = Mutex::new(Durum::BOS);
static IPTAL: AtomicBool = AtomicBool::new(false);
static DURAKLAT: AtomicBool = AtomicBool::new(false);

fn durum(f: impl FnOnce(&mut Durum)) {
    f(&mut DURUM.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Aynı harfin farklı kodlanmış biçimlerini profile göre tek tipe getirir; böylece
/// aramada bu farklar engel olmaz. Osmanlıca (fas içeren) profilde Farsça biçim
/// (ی، ک), diğerlerinde Arapça biçim (ي، ك) esas alınır. Kaşide (ـ) silinir.
/// Hemzeli elifler ve Arapçadaki elif maksure (ى) anlamı etkilediği için korunur.
fn bicim_birligi(metin: &str, dil: &str) -> String {
    let farsca = dil.contains("fas");
    metin
        .chars()
        .filter(|&c| c != '\u{0640}')
        .map(|c| match (farsca, c) {
            (true, '\u{064A}' | '\u{0649}') => '\u{06CC}', // ي ى → ی
            (true, '\u{0643}') => '\u{06A9}',               // ك → ک
            (false, '\u{06CC}') => '\u{064A}',              // ی → ي
            (false, '\u{06A9}') => '\u{0643}',              // ک → ك
            _ => c,
        })
        .collect()
}

/// Tek sayfayı OCR'dan geçirir. Sonuç (oN.tsv) zaten varsa atlanır; böylece
/// yarıda kalan bir iş kaldığı yerden sürer.
fn sayfa_oku(kaynak: &Path, resim: bool, i: usize, is_k: &Path, dil: &str, yon: bool) -> Result<(), String> {
    let taban = is_k.join(format!("o{}", i + 1));
    if taban.with_extension("tsv").exists() {
        return Ok(());
    }
    let t = araclar()?.join("tesseract");
    let mut gecici_goruntu = !resim;
    let mut goruntu = if resim {
        kaynak.to_path_buf()
    } else {
        let png = is_k.join(format!("s{}.png", i + 1));
        mutool(&[
            "draw".into(),
            "-r".into(),
            "300".into(),
            "-c".into(),
            "gray".into(),
            "-o".into(),
            png.clone().into(),
            kaynak.into(),
            (i + 1).to_string().into(),
        ])?;
        png
    };
    if resim {
        if let Some(b) = buyut(&goruntu, &is_k.join(format!("b{}.png", i + 1)))? {
            goruntu = b;
            gecici_goruntu = true;
        }
    }
    // Ters ya da yan duran sayfa: OCR görüntüsü döndürülür, açı rN dosyasına yazılır
    // (PDF'teki sayfa da aynı açıyla döndürülecek)
    if yon {
        let r = is_k.join(format!("r{}", i + 1));
        let aci = match std::fs::read_to_string(&r) {
            Ok(s) => s.trim().parse::<u32>().unwrap_or(0),
            Err(_) => {
                let a = yon_bul(&goruntu, &t);
                std::fs::write(&r, a.to_string()).map_err(hata)?;
                a
            }
        };
        if aci != 0 {
            let img = image::open(&goruntu).map_err(hata)?;
            let img = match aci {
                90 => img.rotate90(),
                180 => img.rotate180(),
                _ => img.rotate270(),
            };
            let d = is_k.join(format!("d{}.png", i + 1));
            img.save(&d).map_err(hata)?;
            if gecici_goruntu {
                let _ = std::fs::remove_file(&goruntu);
            }
            goruntu = d;
            gecici_goruntu = true;
        }
    }
    // Önce geçici adla yazılır; .tsv en son yerine konduğu için yarım kalan sayfa "bitti" sayılmaz
    let yari = is_k.join(format!("y{}", i + 1));
    calistir(
        &t.join("tesseract.exe"),
        &[
            goruntu.clone().into(),
            yari.clone().into(),
            "-l".into(),
            tesseract_dili(dil).into(),
            "--tessdata-dir".into(),
            t.join("tessdata").into(),
            "tsv".into(),
            "txt".into(),
        ],
    )?;
    // Karışık profilde (ör. Osmanlıca + Türkçe) yanlış dille okunan kelimeler yeniden okunur
    if let Some(yeni) = karisik_ikinci_okuma(&std::fs::read_to_string(yari.with_extension("tsv")).map_err(hata)?, &goruntu, is_k, i, dil, &t)? {
        std::fs::write(yari.with_extension("txt"), tsv_metni(&yeni)).map_err(hata)?;
        std::fs::write(yari.with_extension("tsv"), yeni).map_err(hata)?;
    }
    // Arayüzde son okunan sayfa olarak gösterilen küçük görüntü
    if let Ok(img) = image::open(&goruntu) {
        let _ = img.thumbnail(700, 1000).to_luma8().save(is_k.join(format!("on{}.jpg", i + 1)));
    }
    for uzanti in ["txt", "tsv"] {
        let y = yari.with_extension(uzanti);
        let metin = std::fs::read_to_string(&y).map_err(hata)?;
        std::fs::write(&y, bicim_birligi(&metin, dil)).map_err(hata)?;
        std::fs::rename(&y, taban.with_extension(uzanti)).map_err(hata)?;
    }
    if gecici_goruntu {
        let _ = std::fs::remove_file(goruntu);
    }
    Ok(())
}

/// Osmanlıca profillerinde Arapça için "best", Farsça için "fast" model kullanılır.
/// Ölçüm (osmanlica.com matbu nesih, 19 sayfa): CER %24,3 → %21,4, süre +%22
/// (ikisi de "best" olsaydı %21,0, süre +%50). Arapça profili standart modelde kalır.
fn tesseract_dili(dil: &str) -> String {
    if dil.contains("fas") {
        dil.replace("ara", "ara_best").replace("fas", "fas_fast")
    } else {
        dil.to_string()
    }
}

/// Yazısı küçük kalan düşük çözünürlüklü sayfa görüntüsünü (uzun kenar < 1600 px, satır yüksekliği
/// < 25 px) uzun kenarı 2200 px olacak şekilde (en çok 3 kat) büyütür; satır şeridi gibi dar görüntülere dokunmaz.
/// Ölçüm: KITAB-Bench Hindawi (612×792 px, satır ~16 px) Arapça CER %30,7 → %11,1. Yazısı zaten
/// yeterince büyük sayfalarda (osmanlica.com matbu nesih, satır ~40 px) büyütme CER'i %24,3 → %25,4 kötüleştirdiği
/// için satır yüksekliği şartı vardır.
fn buyut(yol: &Path, hedef: &Path) -> Result<Option<PathBuf>, String> {
    let (w, h) = image::image_dimensions(yol).map_err(hata)?;
    if w.max(h) >= 1600 || w.min(h) < 400 {
        return Ok(None);
    }
    let img = image::open(yol).map_err(hata)?.to_luma8();
    if !(1..25).contains(&satir_yuksekligi(&img)) {
        return Ok(None);
    }
    let k = (2200.0 / w.max(h) as f32).min(3.0);
    let (yw, yh) = ((w as f32 * k) as u32, (h as f32 * k) as u32);
    image::imageops::resize(&img, yw, yh, image::imageops::FilterType::Triangle).save(hedef).map_err(hata)?;
    Ok(Some(hedef.to_path_buf()))
}

/// Metin satırlarının ortanca yüksekliği (px): yatay izdüşümde koyu piksel içeren ardışık satır kümeleri
fn satir_yuksekligi(img: &image::GrayImage) -> u32 {
    let ort = img.as_raw().iter().map(|&p| p as u64).sum::<u64>() / (img.as_raw().len() as u64).max(1);
    let (esik, sinir) = ((ort * 3 / 4) as u8, (img.width() / 200).max(2) as usize);
    let (mut kosular, mut n) = (Vec::new(), 0);
    for satir in img.rows().map(Some).chain([None]) {
        if satir.is_some_and(|s| s.filter(|p| p.0[0] < esik).count() > sinir) {
            n += 1;
        } else {
            if n >= 4 {
                kosular.push(n);
            }
            n = 0;
        }
    }
    kosular.sort_unstable();
    kosular.get(kosular.len() / 2).copied().unwrap_or(0)
}

/// Kelimenin yazı sistemi: Some(true) Arap harfli, Some(false) Latin harfli, None belirsiz (ör. sayı)
fn arap_harfli(s: &str) -> Option<bool> {
    let arap = s
        .chars()
        .filter(|c| !c.is_numeric() && matches!(*c as u32, 0x0600..=0x06FF | 0x0750..=0x077F | 0xFB50..=0xFEFF))
        .count();
    let latin = s.chars().filter(|c| c.is_alphabetic() && (*c as u32) < 0x0250).count();
    match arap.cmp(&latin) {
        std::cmp::Ordering::Greater => Some(true),
        std::cmp::Ordering::Less => Some(false),
        std::cmp::Ordering::Equal => None,
    }
}

/// Karışık profillerde, aynı satırda yakın komşularının çoğu başka yazı sisteminde olan kelime
/// (Osmanlıca satırda Latin harfli kelime gibi) yanlış modelle okunmuştur:
/// başka bir kelimenin üstüne düşen kopyaysa silinir, değilse diğer modelle yeniden okunur.
/// Değişiklik olursa yeni TSV'yi döndürür.
fn karisik_ikinci_okuma(tsv: &str, goruntu: &Path, is_k: &Path, i: usize, dil: &str, t: &Path) -> Result<Option<String>, String> {
    let latin_mi = |d: &&str| matches!(*d, "tur" | "eng" | "deu");
    let arap_dili = tesseract_dili(&dil.split('+').filter(|d| !latin_mi(d)).collect::<Vec<_>>().join("+"));
    let latin_dili = dil.split('+').filter(latin_mi).collect::<Vec<_>>().join("+");
    if arap_dili.is_empty() || latin_dili.is_empty() {
        return Ok(None);
    }
    let mut satirlar: Vec<Vec<String>> = tsv.lines().map(|l| l.split('\t').map(str::to_string).collect()).collect();
    let n = |r: &[String], k: usize| r.get(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    let ortak = |a0: f64, a1: f64, b0: f64, b1: f64| a1.min(b1) - a0.max(b0);
    // Kelimeler: (satır dizini, Arap harfli mi, x0, y0, x1, y1)
    let kelimeler: Vec<(usize, bool, f64, f64, f64, f64)> = satirlar
        .iter()
        .enumerate()
        .filter(|(_, r)| r.len() >= 12 && r[0] == "5")
        .filter_map(|(j, r)| arap_harfli(r[11].trim()).map(|a| (j, a, n(r, 6), n(r, 7), n(r, 6) + n(r, 8), n(r, 7) + n(r, 9))))
        .collect();
    let mut sil = Vec::new();
    let mut oku = Vec::new();
    for &(j, a, x0, y0, x1, y1) in &kelimeler {
        let h = y1 - y0;
        let (mut arap, mut latin, mut ustunde) = (0, 0, false);
        for &(k, ka, kx0, ky0, kx1, ky1) in &kelimeler {
            if k == j || ortak(y0, y1, ky0, ky1) < 0.5 * h.min(ky1 - ky0) {
                continue;
            }
            if ortak(x0, x1, kx0, kx1) >= 0.5 * (x1 - x0).min(kx1 - kx0) {
                ustunde |= ka != a;
            } else if (kx0 - x1).max(x0 - kx1) <= 5.0 * h {
                if ka { arap += 1 } else { latin += 1 }
            }
        }
        if (a && latin > arap) || (!a && arap > latin) {
            if ustunde { sil.push(j) } else { oku.push((j, a)) }
        }
    }
    if sil.is_empty() && oku.is_empty() {
        return Ok(None);
    }
    for j in sil {
        satirlar[j][11].clear();
    }
    let img = image::open(goruntu).map_err(hata)?;
    let parca = is_k.join(format!("k{}.png", i + 1));
    for (j, arap) in oku {
        let r = &satirlar[j];
        let (x, y) = ((n(r, 6) - 4.0).max(0.0) as u32, (n(r, 7) - 4.0).max(0.0) as u32);
        img.crop_imm(x, y, n(r, 8) as u32 + 8, n(r, 9) as u32 + 8).save(&parca).map_err(hata)?;
        let cikti = calistir(
            &t.join("tesseract.exe"),
            &[
                parca.clone().into(),
                "stdout".into(),
                "-l".into(),
                (if arap { &latin_dili } else { &arap_dili }).into(),
                "--psm".into(),
                "8".into(),
                "--tessdata-dir".into(),
                t.join("tessdata").into(),
                "tsv".into(),
            ],
        )
        .unwrap_or_default();
        let (mut metin, mut guven, mut adet) = (Vec::new(), 0.0, 0.0);
        for c in cikti.lines().map(|l| l.split('\t').collect::<Vec<_>>()) {
            if c.len() >= 12 && c[0] == "5" && !c[11].trim().is_empty() {
                metin.push(c[11].trim());
                guven += c[10].parse::<f64>().unwrap_or(0.0);
                adet += 1.0;
            }
        }
        let metin = metin.join(" ");
        // Osmanlıca satırdaki Latin kelime Arap harfli okunduysa kabul edilir;
        // Latin satırdaki Arap harfli kelime ise ancak daha güvenle okunduysa değiştirilir
        let kabul = match arap_harfli(&metin) {
            Some(true) => !arap,
            Some(false) => arap && guven / adet > n(r, 10),
            None => false,
        };
        if kabul {
            satirlar[j][11] = metin;
            satirlar[j][10] = "50".into(); // düzeltme ekranında işaretli görünsün
        }
    }
    let _ = std::fs::remove_file(&parca);
    Ok(Some(satirlar.iter().map(|r| r.join("\t")).collect::<Vec<_>>().join("\n")))
}

/// TSV'den düz metin: satırlar alt alta, paragraflar arasında boş satır, sayfa sonunda \f
fn tsv_metni(tsv: &str) -> String {
    let mut metin = String::new();
    let (mut son_satir, mut son_paragraf) = (String::new(), String::new());
    for r in tsv.lines() {
        let c: Vec<&str> = r.split('\t').collect();
        if c.len() < 12 || c[0] != "5" || c[11].trim().is_empty() {
            continue;
        }
        let (paragraf, satir) = (format!("{}.{}", c[2], c[3]), format!("{}.{}.{}", c[2], c[3], c[4]));
        if satir != son_satir && !metin.is_empty() {
            metin += if paragraf != son_paragraf { "\n\n" } else { "\n" };
        } else if !metin.is_empty() {
            metin.push(' ');
        }
        metin += c[11].trim();
        (son_satir, son_paragraf) = (satir, paragraf);
    }
    metin + "\n\u{c}"
}

/// Tesseract'ın yön algılamasıyla sayfanın kaç derece (saat yönünde) döndürülmesi
/// gerektiğini bulur. Emin olunamazsa 0 döner; yanlış döndürme, döndürmemekten kötüdür.
fn yon_bul(goruntu: &Path, t: &Path) -> u32 {
    let Ok(cikti) = calistir(
        &t.join("tesseract.exe"),
        &[
            goruntu.into(),
            "stdout".into(),
            "--psm".into(),
            "0".into(),
            "--tessdata-dir".into(),
            t.join("tessdata").into(),
        ],
    ) else {
        return 0;
    };
    let deger = |ad: &str| cikti.lines().find_map(|l| l.strip_prefix(ad)).and_then(|v| v.trim().parse::<f64>().ok());
    match (deger("Rotate:"), deger("Orientation confidence:")) {
        (Some(aci), Some(guven)) if guven >= 5.0 => (aci as u32) % 360,
        _ => 0,
    }
}

#[derive(Clone, serde::Deserialize)]
struct OcrAyar {
    dil: String,
    cekirdek: usize,
    atla: bool,
    sirala: String,
    pdfa: bool,
    yon: bool,
    cift: bool,
    word: bool,
    epub: bool,
}

fn ocr_dosyasi(
    orijinal: &Path,
    a: &OcrAyar,
) -> Result<(PathBuf, Option<serde_json::Value>, Option<&'static str>), String> {
    let (dil, cekirdek, atla, sirala, pdfa) = (a.dil.as_str(), a.cekirdek, a.atla, a.sirala.as_str(), a.pdfa);
    let (mut kaynak, resim) = girdi(orijinal)?;
    let is_k = klasor(gecici().join("isler").join(ozet(orijinal, &format!("{dil}{}{}", a.yon, a.cift))))?;

    // Çift sayfalı (yatay) taramalar, görüntüye dokunmadan görünür alanı ikiye ayrılarak bölünür
    let mut bolunmus = None;
    if a.cift && !resim {
        let hedef = is_k.join("bolunmus.pdf");
        if hedef.exists() || cift_sayfa_bol(&kaynak, &hedef, dil.contains("ara"))? {
            kaynak = hedef.clone();
            bolunmus = Some(hedef);
        }
    }

    // Zaten metin içeren sayfalar (atla seçiliyse) OCR'a sokulmaz; mevcut metinleri .txt'ye yazılır
    let metinler: Vec<String> = if resim {
        vec![String::new()]
    } else {
        serde_json::from_str(betik(&["metinler".into(), kaynak.clone().into()])?.trim()).map_err(hata)?
    };
    let metinli: Vec<bool> = metinler.iter().map(|m| atla && !m.trim().is_empty()).collect();
    let toplam = metinli.len();
    durum(|d| {
        d.toplam = toplam;
        d.bitti = metinli.iter().filter(|m| **m).count();
    });

    let kuyruk = Mutex::new((0..toplam).filter(|&i| !metinli[i]).rev().collect::<Vec<_>>());
    let ilk_hata: Mutex<Option<String>> = Mutex::new(None);
    std::thread::scope(|s| {
        for _ in 0..cekirdek.max(1) {
            s.spawn(|| loop {
                while DURAKLAT.load(Relaxed) && !IPTAL.load(Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                }
                if IPTAL.load(Relaxed) || ilk_hata.lock().unwrap().is_some() {
                    return;
                }
                let Some(i) = kuyruk.lock().unwrap().pop() else { return };
                match sayfa_oku(&kaynak, resim, i, &is_k, dil, a.yon) {
                    Ok(()) => durum(|d| {
                        d.bitti += 1;
                        if let Some((k, eski)) = d.son.replace((is_k.clone(), i)) {
                            let _ = std::fs::remove_file(k.join(format!("on{}.jpg", eski + 1)));
                        }
                    }),
                    Err(e) => {
                        *ilk_hata.lock().unwrap() = Some(format!("{}. sayfa: {e}", i + 1));
                        return;
                    }
                }
            });
        }
    });
    if let Some(e) = ilk_hata.into_inner().unwrap() {
        return Err(e);
    }
    if IPTAL.load(Relaxed) {
        return Err("İptal edildi (yeniden başlatırsanız kaldığı yerden sürer)".into());
    }

    let donus: Vec<u32> = (0..toplam)
        .map(|i| {
            std::fs::read_to_string(is_k.join(format!("r{}", i + 1)))
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0)
        })
        .collect();
    let tsvler: Vec<Option<String>> = (0..toplam)
        .map(|i| (!metinli[i]).then(|| is_k.join(format!("o{}.tsv", i + 1)).display().to_string()))
        .collect();
    let cikti = orijinal.with_file_name(format!(
        "{}_varak.pdf",
        orijinal.file_stem().unwrap_or_default().to_string_lossy()
    ));
    let is = serde_json::json!({
        "kaynak": kaynak.display().to_string(),
        "resim": resim,
        "dpi": 300,
        "font": font_yolu().display().to_string(),
        "cikti": (if pdfa { is_k.join("ara.pdf") } else { cikti.clone() }).display().to_string(),
        "tsvler": tsvler,
        "sirala": sirala,
        "donus": donus,
    });
    let is_dosyasi = is_k.join("is.json");
    std::fs::write(&is_dosyasi, is.to_string()).map_err(hata)?;
    let rapor: Option<serde_json::Value> =
        serde_json::from_str(betik(&["ocr".into(), is_dosyasi.into()])?.trim()).ok();
    if pdfa {
        pdfa_yap(&is_k.join("ara.pdf"), &cikti, &is_k)?;
    }

    // Metin dosyası da PDF'teki sayfa sırasıyla yazılır
    let sira: Vec<usize> = rapor
        .as_ref()
        .and_then(|r| serde_json::from_value(r["sira"].clone()).ok())
        .unwrap_or_else(|| (0..toplam).collect());
    let mut metin = String::new();
    for &i in &sira {
        match metinli.get(i) {
            Some(false) => metin += &std::fs::read_to_string(is_k.join(format!("o{}.txt", i + 1))).map_err(hata)?,
            Some(true) => {
                metin += &metinler[i];
                metin.push('\u{c}'); // Tesseract gibi sayfa sonu karakteri
            }
            None => {}
        }
    }
    std::fs::write(cikti.with_extension("txt"), metin).map_err(hata)?;
    if let Some(r) = &rapor {
        ek_ciktilar(&cikti, r, dil, a.word, a.epub)?;
    }
    duzeltme_verisi_sakla(&cikti, orijinal, a, resim, &sira, &metinli, &donus, bolunmus.as_deref(), &is_k)?;
    let _ = std::fs::remove_dir_all(&is_k);
    let not = (toplam > 0 && metinli.iter().all(|m| *m))
        .then_some("Tüm sayfalar zaten metin içeriyordu; OCR yapılmadı, mevcut metin kullanıldı");
    Ok((cikti, rapor, not))
}

// ---------- Word ve EPUB ----------

/// Sayfa sayfa paragraflar: (metin, sağdan sola mı)
type Sayfalar = Vec<Vec<(String, bool)>>;

fn ek_ciktilar(cikti: &Path, rapor: &serde_json::Value, dil: &str, word: bool, epub: bool) -> Result<(), String> {
    if !word && !epub {
        return Ok(());
    }
    let sayfalar: Sayfalar = rapor["paragraflar"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|s| {
            s.as_array()
                .into_iter()
                .flatten()
                .map(|p| (p["m"].as_str().unwrap_or_default().to_string(), p["rtl"].as_bool().unwrap_or(false)))
                .collect()
        })
        .collect();
    if word {
        docx_yaz(&cikti.with_extension("docx"), &sayfalar)?;
    }
    if epub {
        let baslik = cikti.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        epub_yaz(&cikti.with_extension("epub"), &sayfalar, dil, &baslik)?;
    }
    Ok(())
}

/// XML'de özel anlamı olan karakterleri kaçırır; XML'de geçersiz denetim karakterlerini atar
fn xml(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\t')
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn zip_yaz(yol: &Path, dosyalar: &[(String, String)]) -> Result<(), String> {
    use std::io::Write;
    let mut z = zip::ZipWriter::new(std::fs::File::create(yol).map_err(hata)?);
    for (i, (ad, icerik)) in dosyalar.iter().enumerate() {
        // EPUB'da ilk dosya (mimetype) sıkıştırılmadan yazılmalı
        let yontem = if i == 0 && ad == "mimetype" { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
        z.start_file(ad.as_str(), zip::write::SimpleFileOptions::default().compression_method(yontem))
            .map_err(hata)?;
        z.write_all(icerik.as_bytes()).map_err(hata)?;
    }
    z.finish().map_err(hata)?;
    Ok(())
}

const DOCX_TURLER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;

const DOCX_ILISKILER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;

/// Her sayfa yeni bir Word sayfasından başlar; sağdan sola paragraflar bidi olarak işaretlenir
fn docx_yaz(yol: &Path, sayfalar: &Sayfalar) -> Result<(), String> {
    let mut govde = String::new();
    for (i, sayfa) in sayfalar.iter().enumerate() {
        let bos = vec![(String::new(), false)];
        let paragraflar = if sayfa.is_empty() { &bos } else { sayfa };
        for (j, (metin, rtl)) in paragraflar.iter().enumerate() {
            govde += &format!(
                "<w:p><w:pPr>{}{}</w:pPr><w:r><w:rPr>{}</w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                if i > 0 && j == 0 { "<w:pageBreakBefore/>" } else { "" },
                if *rtl { "<w:bidi/>" } else { "" },
                if *rtl { "<w:rtl/>" } else { "" },
                xml(metin)
            );
        }
    }
    let belge = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{govde}\
         <w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1418\" w:right=\"1418\" \
         w:bottom=\"1418\" w:left=\"1418\" w:header=\"709\" w:footer=\"709\" w:gutter=\"0\"/></w:sectPr>\
         </w:body></w:document>"
    );
    zip_yaz(
        yol,
        &[
            ("[Content_Types].xml".into(), DOCX_TURLER.into()),
            ("_rels/.rels".into(), DOCX_ILISKILER.into()),
            ("word/document.xml".into(), belge),
        ],
    )
}

fn dil_kodu(dil: &str) -> &'static str {
    match dil {
        "ara" => "ar",
        "eng" => "en",
        "deu" => "de",
        _ if dil.contains("fas") => "ota",
        _ => "tr",
    }
}

/// EPUB 3'ün istediği değişiklik zamanı (UTC, ör. 2026-11-16T12:30:00Z)
fn utc_zaman() -> String {
    let sn = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (gun, s) = (sn.div_euclid(86400), sn.rem_euclid(86400));
    // Gün sayısından takvim tarihine (Howard Hinnant'ın algoritması)
    let z = gun + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let g = doy - (153 * mp + 2) / 5 + 1;
    let a = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(a <= 2);
    format!("{y:04}-{a:02}-{g:02}T{:02}:{:02}:{:02}Z", s / 3600, s % 3600 / 60, s % 60)
}

/// Her sayfa bir bölüm olur; çoğunlukla sağdan sola yazılan kitaplarda sayfa çevirme yönü de sağdan soladır
fn epub_yaz(yol: &Path, sayfalar: &Sayfalar, dil: &str, baslik: &str) -> Result<(), String> {
    let kod = dil_kodu(dil);
    let paragraflar = sayfalar.iter().flatten();
    let rtl = paragraflar.clone().filter(|p| p.1).count() * 2 > paragraflar.count();
    let mut dosyalar: Vec<(String, String)> = vec![
        ("mimetype".into(), "application/epub+zip".into()),
        (
            "META-INF/container.xml".into(),
            r#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.into(),
        ),
    ];
    let (mut ogeler, mut sira, mut icindekiler) = (String::new(), String::new(), String::new());
    for (i, sayfa) in sayfalar.iter().enumerate() {
        let n = i + 1;
        let govde: String = sayfa
            .iter()
            .map(|(m, r)| format!("<p{}>{}</p>", if *r { " dir=\"rtl\"" } else { "" }, xml(m)))
            .collect();
        dosyalar.push((
            format!("OEBPS/s{n}.xhtml"),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE html><html xmlns=\"http://www.w3.org/1999/xhtml\" \
                 lang=\"{kod}\" xml:lang=\"{kod}\"><head><title>Sayfa {n}</title></head><body>{govde}</body></html>"
            ),
        ));
        ogeler += &format!("<item id=\"s{n}\" href=\"s{n}.xhtml\" media-type=\"application/xhtml+xml\"/>");
        sira += &format!("<itemref idref=\"s{n}\"/>");
        icindekiler += &format!("<li><a href=\"s{n}.xhtml\">Sayfa {n}</a></li>");
    }
    dosyalar.push((
        "OEBPS/nav.xhtml".into(),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><!DOCTYPE html><html xmlns=\"http://www.w3.org/1999/xhtml\" \
             xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>İçindekiler</title></head><body>\
             <nav epub:type=\"toc\"><ol>{icindekiler}</ol></nav></body></html>"
        ),
    ));
    let kimlik = ozet(yol, baslik);
    dosyalar.push((
        "OEBPS/content.opf".into(),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" \
             unique-identifier=\"kimlik\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
             <dc:identifier id=\"kimlik\">urn:varak:{kimlik}</dc:identifier><dc:title>{}</dc:title>\
             <dc:language>{kod}</dc:language><meta property=\"dcterms:modified\">{}</meta></metadata><manifest>\
             <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>{ogeler}\
             </manifest><spine{}>{sira}</spine></package>",
            xml(baslik),
            utc_zaman(),
            if rtl { " page-progression-direction=\"rtl\"" } else { "" }
        ),
    ));
    zip_yaz(yol, &dosyalar)
}

/// Yatay sayfaları iki yarıya bölen ara PDF'i üretir. Bölünecek sayfa yoksa false döner.
/// Sağdan sola yazılan eserlerde (rtl) önce sağdaki yarı gelir.
fn cift_sayfa_bol(kaynak: &Path, hedef: &Path, rtl: bool) -> Result<bool, String> {
    let boyutlar: Vec<[f64; 3]> =
        serde_json::from_str(betik(&["boyutlar".into(), kaynak.into()])?.trim()).map_err(hata)?;
    let yatay = |b: &[f64; 3]| b[2] == 0.0 && b[0] > b[1] * 1.1;
    if !boyutlar.iter().any(yatay) {
        return Ok(false);
    }
    let k = kaynak.display().to_string();
    let mut sayfalar = Vec::new();
    for (i, b) in boyutlar.iter().enumerate() {
        if !yatay(b) {
            sayfalar.push(serde_json::json!({ "kaynak": k, "no": i }));
            continue;
        }
        // kırpma oranları: sol, üst, sağ, alt
        let sol = serde_json::json!({ "kaynak": k, "no": i, "kirp": [0.0, 0.0, 0.5, 0.0] });
        let sag = serde_json::json!({ "kaynak": k, "no": i, "kirp": [0.5, 0.0, 0.0, 0.0] });
        if rtl {
            sayfalar.extend([sag, sol]);
        } else {
            sayfalar.extend([sol, sag]);
        }
    }
    let plan = serde_json::json!({ "a4": false, "ciktilar": [{ "dosya": hedef.display().to_string(), "sayfalar": sayfalar }] });
    let yol = hedef.with_extension("json");
    std::fs::write(&yol, plan.to_string()).map_err(hata)?;
    betik(&["uygula".into(), yol.into()])?;
    Ok(true)
}

/// Ghostscript ile PDF/A-2b üretir. Çıktı amacı (OutputIntent) için Windows'un
/// kendi sRGB renk profili kullanılır.
fn pdfa_yap(girdi: &Path, cikti: &Path, is_k: &Path) -> Result<(), String> {
    let renk = PathBuf::from(std::env::var_os("WINDIR").unwrap_or("C:\\Windows".into()))
        .join("System32")
        .join("spool")
        .join("drivers")
        .join("color");
    let ps = |p: &Path| p.display().to_string().replace('\\', "/");
    let tanim = format!(
        "%!\n/ICCProfile ({}) def\n\
         [/_objdef {{icc_PDFA}} /type /stream /OBJ pdfmark\n\
         [{{icc_PDFA}} << /N 3 >> /PUT pdfmark\n\
         [{{icc_PDFA}} ICCProfile (r) file /PUT pdfmark\n\
         [/_objdef {{OutputIntent_PDFA}} /type /dict /OBJ pdfmark\n\
         [{{OutputIntent_PDFA}} << /Type /OutputIntent /S /GTS_PDFA1 /DestOutputProfile {{icc_PDFA}} \
         /OutputConditionIdentifier (sRGB) >> /PUT pdfmark\n\
         [{{Catalog}} << /OutputIntents [ {{OutputIntent_PDFA}} ] >> /PUT pdfmark\n",
        ps(&renk.join("sRGB Color Space Profile.icm"))
    );
    let tanim_dosyasi = is_k.join("PDFA_def.ps");
    std::fs::write(&tanim_dosyasi, tanim).map_err(hata)?;
    calistir(
        &araclar()?.join("gs").join("bin").join("gswin64c.exe"),
        &[
            "-dPDFA=2".into(),
            "-dBATCH".into(),
            "-dNOPAUSE".into(),
            "-dQUIET".into(),
            "-sDEVICE=pdfwrite".into(),
            "-sColorConversionStrategy=RGB".into(),
            "-dPDFACompatibilityPolicy=1".into(),
            format!("--permit-file-read={}/", ps(&renk)).into(),
            format!("-sOutputFile={}", cikti.display()).into(),
            tanim_dosyasi.into(),
            girdi.into(),
        ],
    )
    .map(|_| ())
}

fn profil_adi(dil: &str) -> &str {
    match dil {
        "tur" => "Türkçe",
        "ara+fas" => "Osmanlıca (matbu)",
        "ara+fas+tur" => "Osmanlıca + Türkçe (karışık)",
        "ara" => "Arapça",
        "tur+ara" => "Türkçe + Arapça",
        "eng" => "İngilizce",
        "deu" => "Almanca",
        "tur+eng" => "Türkçe + İngilizce",
        _ => dil,
    }
}

#[tauri::command]
fn ocr_baslat(dosyalar: Vec<String>, ayar: OcrAyar) -> Result<(), String> {
    {
        let mut d = DURUM.lock().unwrap_or_else(|e| e.into_inner());
        if d.calisiyor {
            return Err("Bir OCR işi zaten sürüyor".into());
        }
        *d = Durum { calisiyor: true, dosya_sayisi: dosyalar.len(), ..Durum::BOS };
    }
    IPTAL.store(false, Relaxed);
    DURAKLAT.store(false, Relaxed);
    std::thread::spawn(move || {
        for (i, y) in dosyalar.iter().enumerate() {
            if IPTAL.load(Relaxed) {
                break;
            }
            let yol = PathBuf::from(y);
            let ad = dosya_adi(&yol);
            durum(|d| {
                d.dosya = ad.clone();
                d.dosya_no = i + 1;
                d.bitti = 0;
                d.toplam = 0;
            });
            let sonuc = match ocr_dosyasi(&yol, &ayar) {
                Ok((c, rapor, not)) => Sonuc {
                    mesaj: format!(
                        "✓ {} ({}){}",
                        c.display(),
                        profil_adi(&ayar.dil),
                        not.map(|n| format!(" — {n}")).unwrap_or_default()
                    ),
                    dosya: Some(c.display().to_string()),
                    rapor,
                },
                Err(e) => Sonuc { mesaj: format!("✗ {ad}: {e}"), dosya: None, rapor: None },
            };
            durum(|d| d.sonuclar.push(sonuc));
        }
        durum(|d| d.calisiyor = false);
    });
    Ok(())
}

/// İşin durumu; son okunan sayfanın küçük görüntüsü ve satırları da eklenir (arayüzde canlı önizleme)
#[tauri::command]
fn ocr_durum() -> serde_json::Value {
    let d = DURUM.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut v = serde_json::to_value(&d).unwrap_or_default();
    if let Some((is_k, i)) = &d.son {
        let resim = is_k.join(format!("on{}.jpg", i + 1));
        if let (true, Ok(tsv)) = (resim.exists(), std::fs::read_to_string(is_k.join(format!("o{}.tsv", i + 1)))) {
            let (w, h, satirlar) = tsv_satirlari(&tsv);
            v["son"] = serde_json::json!({
                "sayfa": i + 1,
                "resim": resim.display().to_string(),
                "W": w,
                "H": h,
                "satirlar": satirlar.iter().map(|l| serde_json::json!({
                    "x": l.x0, "y": l.y0, "w": l.x1 - l.x0, "h": l.y1 - l.y0,
                    "metin": l.kelimeler.iter().map(|k| k.0.as_str()).collect::<Vec<_>>().join(" "),
                })).collect::<Vec<_>>(),
            });
        }
    }
    v
}

/// Dosyayı ya da klasörü Windows'ta varsayılan programıyla açar; göreli yol uygulama klasörüne göredir
#[tauri::command]
fn ac(yol: String) -> Result<(), String> {
    let p = araclar()?.parent().ok_or("Uygulama klasörü bulunamadı")?.join(&yol);
    if !p.exists() {
        return Err(format!("Bulunamadı: {}", p.display()));
    }
    Command::new("explorer").arg(&p).spawn().map_err(hata)?;
    Ok(())
}

#[tauri::command]
fn ocr_duraklat(durakla: bool) {
    DURAKLAT.store(durakla, Relaxed);
}

#[tauri::command]
fn ocr_iptal() {
    IPTAL.store(true, Relaxed);
}

// ---------- Düzeltme ----------

/// Bir OCR çıktısının düzeltme verisinin kalıcı klasörü (%LOCALAPPDATA%\Varak\duzeltme\...)
fn veri_klasoru(cikti: &Path) -> PathBuf {
    let mut h = DefaultHasher::new();
    cikti.hash(&mut h);
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| std::env::temp_dir().into_os_string()))
        .join("Varak")
        .join("duzeltme")
        .join(format!("{:x}", h.finish()))
}

fn json_oku(yol: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(yol).ok()?).ok()
}

/// OCR sonucundaki sayfa verilerini (TSV) PDF'teki sayfa sırasıyla saklar
#[allow(clippy::too_many_arguments)]
fn duzeltme_verisi_sakla(
    cikti: &Path,
    orijinal: &Path,
    a: &OcrAyar,
    resim: bool,
    sira: &[usize],
    metinli: &[bool],
    donus: &[u32],
    bolunmus: Option<&Path>,
    is_k: &Path,
) -> Result<(), String> {
    let veri = veri_klasoru(cikti);
    let _ = std::fs::remove_dir_all(&veri);
    let veri = klasor(veri)?;
    let mut sayfalar = Vec::new();
    for (k, &i) in sira.iter().enumerate() {
        if metinli[i] {
            sayfalar.push(serde_json::Value::Null);
            continue;
        }
        let ad = format!("p{}.tsv", k + 1);
        std::fs::copy(is_k.join(format!("o{}.tsv", i + 1)), veri.join(&ad)).map_err(hata)?;
        sayfalar.push(ad.into());
    }
    let mut is = serde_json::json!({
        "orijinal": orijinal.display().to_string(),
        "resim": resim,
        "dil": a.dil,
        "pdfa": a.pdfa,
        "word": a.word,
        "epub": a.epub,
        "sira": sira,
        "donus": donus,
        "sayfalar": sayfalar,
    });
    // Çift sayfalar bölündüyse OCR'ın yapıldığı ara PDF de saklanır
    if let Some(b) = bolunmus {
        std::fs::copy(b, veri.join("kaynak.pdf")).map_err(hata)?;
        is["kaynak_pdf"] = "kaynak.pdf".into();
    }
    std::fs::write(veri.join("is.json"), is.to_string()).map_err(hata)
}

/// Düzeltme verisini ve kaynak dosyayı yükler
fn duzeltme_isi(cikti: &str) -> Result<(PathBuf, serde_json::Value, PathBuf), String> {
    let veri = veri_klasoru(Path::new(cikti));
    let is = json_oku(&veri.join("is.json"))
        .ok_or("Bu dosya için düzeltme verisi yok. Dosyayı Varak 0.8 ya da sonraki bir sürümle OCR'dan geçirin.")?;
    if let Some(k) = is["kaynak_pdf"].as_str().map(|k| veri.join(k)) {
        return Ok((veri, is, k));
    }
    let orijinal = PathBuf::from(is["orijinal"].as_str().unwrap_or_default());
    if !orijinal.exists() {
        return Err(format!("Kaynak dosya bulunamadı: {}", orijinal.display()));
    }
    Ok((veri, is, girdi(&orijinal)?.0))
}

struct Satir {
    anahtar: String,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    kelimeler: Vec<(String, f64, [f64; 4])>, // metin, güven, kutu (x, y, genişlik, yükseklik)
}

/// TSV'yi satırlara ayırır; sayfanın piksel genişliğini ve yüksekliğini de döndürür
fn tsv_satirlari(tsv: &str) -> (f64, f64, Vec<Satir>) {
    let (mut w, mut h) = (0.0f64, 0.0f64);
    let mut satirlar: Vec<Satir> = Vec::new();
    for r in tsv.lines() {
        let c: Vec<&str> = r.split('\t').collect();
        if c.len() < 12 {
            continue;
        }
        let n = |i: usize| c[i].parse::<f64>().unwrap_or(0.0);
        if c[0] == "1" {
            w = n(8);
            h = n(9);
        }
        let metin = c[11].trim();
        if c[0] != "5" || metin.is_empty() {
            continue;
        }
        let anahtar = format!("{}.{}.{}", c[2], c[3], c[4]);
        if satirlar.last().map_or(true, |l| l.anahtar != anahtar) {
            satirlar.push(Satir { anahtar, x0: f64::MAX, y0: f64::MAX, x1: 0.0, y1: 0.0, kelimeler: Vec::new() });
        }
        let l = satirlar.last_mut().unwrap();
        l.x0 = l.x0.min(n(6));
        l.y0 = l.y0.min(n(7));
        l.x1 = l.x1.max(n(6) + n(8));
        l.y1 = l.y1.max(n(7) + n(9));
        l.kelimeler.push((metin.to_string(), n(10), [n(6), n(7), n(8), n(9)]));
    }
    (w, h, satirlar)
}

#[tauri::command]
async fn duzeltme_ac(cikti: String) -> Result<serde_json::Value, String> {
    let (veri, is, _) = duzeltme_isi(&cikti)?;
    let sayfalar = is["sayfalar"].as_array().cloned().unwrap_or_default();
    // Güveni 60'ın altında kelime içeren sayfalar
    let dusuk: Vec<usize> = sayfalar
        .iter()
        .enumerate()
        .filter(|(_, ad)| {
            ad.as_str()
                .and_then(|a| std::fs::read_to_string(veri.join(a)).ok())
                .is_some_and(|t| tsv_satirlari(&t).2.iter().any(|l| l.kelimeler.iter().any(|k| k.1 < 60.0)))
        })
        .map(|(k, _)| k)
        .collect();
    Ok(serde_json::json!({ "toplam": sayfalar.len(), "dusuk": dusuk }))
}

#[tauri::command]
async fn duzeltme_sayfa(cikti: String, k: usize) -> Result<serde_json::Value, String> {
    let (veri, is, kaynak) = duzeltme_isi(&cikti)?;
    let Some(ad) = is["sayfalar"][k].as_str() else {
        return Ok(serde_json::json!({ "atlandi": true }));
    };
    let (w, h, satirlar) = tsv_satirlari(&std::fs::read_to_string(veri.join(ad)).map_err(hata)?);
    let i = is["sira"][k].as_u64().unwrap_or(k as u64);
    let sayfa = i + 1;
    let png = klasor(gecici().join("duzelt"))?.join(format!("{}_{}.png", ozet(Path::new(&cikti), ""), k + 1));
    mutool(&[
        "draw".into(),
        "-R".into(),
        is["donus"][i as usize].as_u64().unwrap_or(0).to_string().into(),
        "-w".into(),
        "2000".into(), // yakınlaştırınca da harfler net görünsün
        "-o".into(),
        png.clone().into(),
        kaynak.into(),
        sayfa.to_string().into(),
    ])?;
    let duzeltme = json_oku(&veri.join("duzeltme.json")).map(|d| d[k.to_string()].clone()).unwrap_or_default();
    Ok(serde_json::json!({
        "goruntu": png.display().to_string(),
        "W": w,
        "H": h,
        "duzeltme": duzeltme,
        "satirlar": satirlar.iter().map(|l| serde_json::json!({
            "anahtar": l.anahtar,
            "x": l.x0,
            "y": l.y0,
            "w": l.x1 - l.x0,
            "h": l.y1 - l.y0,
            "kelimeler": l.kelimeler.iter().map(|(m, g, k)| serde_json::json!({ "metin": m, "guven": g, "kutu": k })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    }))
}

static DUZELTME_KILIDI: Mutex<()> = Mutex::new(());

/// Bir satırın düzeltmesini kaydeder. metin = null: OCR metni değiştirilmedi.
#[tauri::command]
async fn duzeltme_kaydet(
    cikti: String,
    k: usize,
    anahtar: String,
    metin: Option<String>,
    dogrulandi: bool,
) -> Result<(), String> {
    let _kilit = DUZELTME_KILIDI.lock().unwrap_or_else(|e| e.into_inner());
    let veri = veri_klasoru(Path::new(&cikti));
    let is = json_oku(&veri.join("is.json")).ok_or("Düzeltme verisi bulunamadı")?;
    let dil = is["dil"].as_str().unwrap_or_default();
    let yol = veri.join("duzeltme.json");
    let mut d = json_oku(&yol).unwrap_or_default();
    d[k.to_string()][anahtar] = serde_json::json!({
        "metin": metin.map(|m| bicim_birligi(&m, dil)),
        "dogrulandi": dogrulandi,
    });
    std::fs::write(&yol, d.to_string()).map_err(hata)
}

/// PDF'i ve .txt dosyasını düzeltilmiş metinle yeniden üretir (sayfa sırası ve PDF/A korunur)
#[tauri::command]
async fn duzeltme_uygula(cikti: String) -> Result<String, String> {
    let (veri, is, kaynak) = duzeltme_isi(&cikti)?;
    let cikti = PathBuf::from(cikti);
    let sira: Vec<usize> = serde_json::from_value(is["sira"].clone()).map_err(hata)?;
    let d = json_oku(&veri.join("duzeltme.json")).unwrap_or_default();
    let mut tsvler = vec![serde_json::Value::Null; sira.len()];
    let mut duzeltmeler = vec![serde_json::json!({}); sira.len()];
    for (k, &i) in sira.iter().enumerate() {
        if let Some(ad) = is["sayfalar"][k].as_str() {
            tsvler[i] = veri.join(ad).display().to_string().into();
        }
        for (anahtar, s) in d[k.to_string()].as_object().into_iter().flatten() {
            if let Some(m) = s["metin"].as_str() {
                duzeltmeler[i][anahtar] = m.into();
            }
        }
    }
    let pdfa = is["pdfa"].as_bool().unwrap_or(false);
    let ara = klasor(gecici())?.join("duzeltme_ara.pdf");
    let plan = serde_json::json!({
        "kaynak": kaynak.display().to_string(),
        "resim": is["resim"].as_bool().unwrap_or(false),
        "dpi": 300,
        "font": font_yolu().display().to_string(),
        "cikti": (if pdfa { ara.clone() } else { cikti.clone() }).display().to_string(),
        "tsvler": tsvler,
        "sira": sira,
        "donus": is["donus"],
        "duzeltmeler": duzeltmeler,
    });
    let yol = klasor(gecici())?.join("duzeltme_is.json");
    std::fs::write(&yol, plan.to_string()).map_err(hata)?;
    let rapor: serde_json::Value = serde_json::from_str(betik(&["ocr".into(), yol.into()])?.trim()).map_err(hata)?;
    if pdfa {
        pdfa_yap(&ara, &cikti, &klasor(gecici())?)?;
    }
    std::fs::write(cikti.with_extension("txt"), rapor["metin"].as_str().unwrap_or_default()).map_err(hata)?;
    ek_ciktilar(
        &cikti,
        &rapor,
        is["dil"].as_str().unwrap_or_default(),
        is["word"].as_bool().unwrap_or(false),
        is["epub"].as_bool().unwrap_or(false),
    )?;
    Ok(format!("Düzeltmeler yazıldı: {}", cikti.display()))
}

/// Doğrulanmış satırları Tesseract eğitim biçiminde (satır görüntüsü + .gt.txt) dışa aktarır
#[tauri::command]
async fn egitim_aktar(cikti: String) -> Result<String, String> {
    let (veri, is, kaynak) = duzeltme_isi(&cikti)?;
    let d = json_oku(&veri.join("duzeltme.json")).unwrap_or_default();
    let hedef = rfd::FileDialog::new().pick_folder().ok_or("Klasör seçilmedi")?;
    let ad = Path::new(&cikti).file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let mut sayi = 0;
    for (k, sayfa) in d.as_object().into_iter().flatten() {
        let Ok(k) = k.parse::<usize>() else { continue };
        let Some(tsv_ad) = is["sayfalar"][k].as_str() else { continue };
        let (w, _, satirlar) = tsv_satirlari(&std::fs::read_to_string(veri.join(tsv_ad)).map_err(hata)?);
        let dogrular: Vec<&Satir> =
            satirlar.iter().filter(|l| sayfa[&l.anahtar]["dogrulandi"].as_bool() == Some(true)).collect();
        if dogrular.is_empty() {
            continue;
        }
        // Sayfa, OCR'daki piksel boyutuyla yeniden çizilir; satırlar TSV konumlarından kesilir
        let png = klasor(gecici().join("duzelt"))?.join("egitim.png");
        let i = is["sira"][k].as_u64().unwrap_or(k as u64);
        let sayfa_no = i + 1;
        mutool(&[
            "draw".into(),
            "-R".into(),
            is["donus"][i as usize].as_u64().unwrap_or(0).to_string().into(),
            "-w".into(),
            (w as u32).to_string().into(),
            "-c".into(),
            "gray".into(),
            "-o".into(),
            png.clone().into(),
            kaynak.clone().into(),
            sayfa_no.to_string().into(),
        ])?;
        let goruntu = image::open(&png).map_err(hata)?;
        for l in dogrular {
            let metin = sayfa[&l.anahtar]["metin"].as_str().map(str::to_string).unwrap_or_else(|| {
                l.kelimeler.iter().map(|(m, _, _)| m.as_str()).collect::<Vec<_>>().join(" ")
            });
            let taban = format!("{ad}_s{}_{}", k + 1, l.anahtar.replace('.', "_"));
            goruntu
                .crop_imm(l.x0 as u32, l.y0 as u32, (l.x1 - l.x0) as u32, (l.y1 - l.y0) as u32)
                .save(hedef.join(format!("{taban}.png")))
                .map_err(hata)?;
            std::fs::write(hedef.join(format!("{taban}.gt.txt")), metin).map_err(hata)?;
            sayi += 1;
        }
    }
    Ok(format!("{sayi} doğrulanmış satır dışa aktarıldı: {}", hedef.display()))
}

fn main() {
    // Paralel çalışan Tesseract süreçlerinin her biri tek çekirdek kullansın
    std::env::set_var("OMP_THREAD_LIMIT", "1");
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ac, dosya_sec, dosya_bilgi, onizleme, buyuk_onizleme, kayit_yeri, klasor_sec, uygula, sikistir, numara_sirala,
            ocr_baslat, ocr_durum, ocr_duraklat, ocr_iptal,
            duzeltme_ac, duzeltme_sayfa, duzeltme_kaydet, duzeltme_uygula, egitim_aktar
        ])
        .run(tauri::generate_context!())
        .expect("Varak başlatılamadı");
}
