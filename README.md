# Varak

Taranmış PDF'leri ve görselleri **aranabilir, kopyalanabilir PDF'e** çeviren; PDF sayfalarını
birleştiren, bölen, döndüren, kırpan ve sıkıştıran ücretsiz, açık kaynak, **çevrim dışı** bir
Windows uygulaması.

*Varak is a free, open-source, offline Windows application that turns scanned PDFs and images
into searchable, copyable PDFs (Turkish, Ottoman Turkish, Arabic, English, German) and provides
basic PDF page operations. An English summary is at the end of this page.*

## Özellikler

**OCR**
- Girdi: PDF, JPEG, PNG, TIFF (çok sayfalı dahil), BMP, WebP.
- Diller: Türkçe, Osmanlıca (matbu), Arapça, İngilizce, Almanca ve karışık profiller
  (Osmanlıca + Türkçe, Türkçe + Arapça, Türkçe + İngilizce).
- Metin özgün alfabesinde kalır; çeviri yapılmaz. Arap harfli metin Adobe Reader'da doğru
  yönde seçilir, kopyalanır ve aranır.
- Çıktı: aranabilir PDF (isteğe bağlı PDF/A), düz metin (.txt), Word (.docx), EPUB.
- Sayfa numarasına göre otomatik sıralama; tekrarlanan ve eksik numaraları bildirir.
- Ters ya da yan duran sayfaları düzeltme, çift sayfalı taramaları ikiye bölme.
- Düzeltme ekranı: düşük güvenle okunan kelimeleri işaretler; Arap harfli ekran klavyesi.
- 2000 sayfaya kadar işler; yarıda kalan iş kaldığı yerden sürer.

**PDF işlemleri** (OCR'dan bağımsız)
- Birleştirme, bölme, sayfa silme, sıralama, çıkarma, döndürme, boş sayfa ekleme, kırpma.
- Görselden PDF oluşturma, PDF sıkıştırma.

## İndirme ve kullanım

1. [Releases](../../releases) sayfasından son `Varak-x.y.z.zip` dosyasını indirin.
2. ZIP'teki `Varak` klasörünü bir yere çıkarın ve `Varak.exe`'yi çalıştırın.
3. Gereksinim: Windows 10 ya da 11 (64 bit) ve Microsoft Edge WebView2 Runtime
   (Windows 11'de ve güncel Windows 10'da yüklüdür).

Ayrıntılar ZIP'teki `BENIOKU.txt` dosyasındadır.

## Kaynaktan derleme

Windows'ta [Rust](https://rustup.rs) kurulu olmalıdır.

1. `kurulum.bat`: MuPDF, Tesseract, Ghostscript, dil modelleri, yazı tipleri ve lisans metinlerini indirir.
2. `derle.bat`: Uygulamayı derler; `dagitim\Varak\` klasörünü ve `dagitim\Varak-x.y.z.zip` paketini üretir.

Sürümler GitHub Actions ile depodaki kaynaktan otomatik derlenir (`.github/workflows/derle.yml`).

## Lisans

Tüm Hakkı Saklıdır. Halil KARAKAYA.

Varak, [GNU Affero Genel Kamu Lisansı sürüm 3](LICENSE) (AGPL-3.0) ile lisanslanmıştır ve hiçbir
garanti olmaksızın dağıtılır. Kullanılan bileşenler ve lisansları: [UCUNCU_TARAF.txt](UCUNCU_TARAF.txt).

- [Gizlilik politikası](GIZLILIK.md)
- [Kod imzalama politikası](KOD_IMZALAMA.md)

---

## English summary

Varak is a Windows desktop application (Rust, Tauri) for:

- **OCR:** converts scanned PDFs and images (PDF, JPEG, PNG, multi-page TIFF, BMP, WebP) into
  searchable, copyable PDFs, keeping the original script (no translation). Languages: Turkish,
  Ottoman Turkish (printed), Arabic, English, German and mixed profiles. Optional PDF/A, plain
  text, Word (.docx) and EPUB output; automatic page ordering by printed page numbers; orientation
  correction; splitting of two-page scans; a correction screen with an on-screen Arabic-script keyboard.
- **PDF operations:** merge, split, delete, reorder, extract, rotate, insert blank pages, crop,
  images to PDF, compress.

It works fully offline and collects no data ([privacy policy](GIZLILIK.md)). It bundles MuPDF
(AGPL-3.0), Ghostscript (AGPL-3.0) and Tesseract OCR (Apache-2.0) unmodified. Licensed under
AGPL-3.0. Copyright © Halil KARAKAYA.
