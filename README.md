# DeepNode Copy

**DeepNode Copy**, Windows işletim sistemi için geliştirilmiş, ultra hafif, donanım hızında ve güvenilir bir dosya kopyalama, taşıma ve bütünlük doğrulama yöneticisidir. Klasik **TeraCopy** deneyimini modern Rust çekirdek motoru, SIMD hash algoritmaları, asenkron Win32 Overlapped Direct I/O ve şık Tauri v2 + React arayüzü ile bir üst seviyeye taşır.

---

## ⚡ Temel Mimari ve Özellikler

### 1. Çekirdek Motor (`crates/deepnode-core`)
- **Fiziksel Disk Topoloji Analizi**: Kaynak ve hedefin aynı fiziksel sürücüde olup olmadığını `IOCTL_STORAGE_GET_DEVICE_NUMBER` ile algılar.
  - **Aynı Diskte**: Kafa hareketini (seek time) ve disk boğulmasını önlemek için 4 MB dinamik sıralı blok tamponlama.
  - **Farklı Disklerde**: Eşzamanlı çift tamponlu (Double-Buffering) üretici-tüketici asenkron Overlapped I/O.
- **Direct I/O Desteği**: Windows önbelleğini baypas eden `FILE_FLAG_NO_BUFFERING` ve `FILE_FLAG_WRITE_THROUGH` opsiyonu (4096 bayt sektör hizalı bellek havuzu).
- **Akıllı Hata Kurtarma (Skip & Retry)**: Kilitli veya bozuk dosyalarda tüm transferi kesmez; yapılandırılabilir zaman aşımı ve deneme sayısının ardından dosyayı atlayıp bir sonrakine geçer. Transfer sonunda "Yalnızca Hatalı/Atlananları Tekrar Dene" imkanı sunar.
- **VSS (Volume Shadow Copy) Entegrasyonu**: Başka bir uygulama tarafından kilitlenen dosyaları Windows Gölge Kopya servisi üzerinden otomatik okur.
- **NTFS Nitelikleri & Metadata**:
  - Zaman damgaları (Creation, LastWrite, LastAccess) 100ns hassasiyetinde korunur.
  - Nitelikler (Read-Only, Hidden, System, Archive).
  - NTFS Alternatif Veri Akışları (ADS - Alternate Data Streams, örn: `:Zone.Identifier:$DATA`).
  - Güvenlik izinleri (DACL).

---

### 2. Donanım İvmeli Doğrulama & Hash Motoru
- **xxHash3-64 & xxHash3-128**: AVX2 / SSE2 SIMD desteğiyle RAM hızında (15-30 GB/sn) anlık sağlama toplamı.
- **BLAKE3**: Çok kanallı kriptografik SIMD hash.
- **Standart Algoritmalar**: SHA-256, SHA-1, MD5.
- **Kopya Sonrası Doğrulama**: Windows dosya önbelleğini atlayarak verinin diske fiziksel olarak doğru yazıldığını garantileme.
- **Panodan Tek Tıkla Hash Testi**: Panodaki hash ile dosyayı anında kıyaslama.
- **NTFS ADS Checksum Yazma**: Hesaplanmış hash değerini dosyanın kendi NTFS ADS akışına (`:DeepNodeHash`) gömebilme.
- **Checksum Manifest Dosyaları**: Klasöre otomatik `.hash`, `.md5`, `.sha256` dosyası oluşturma.

---

### 3. Akıllı Çakışma Yönetimi (Conflict Resolution)
Hedefte aynı isimli dosya olduğunda:
- **Üzerine Yaz (Overwrite)** / **Tümünün Üzerine Yaz**
- **Atla (Skip)** / **Tümünü Atla**
- **İkisini de Tut / Yeniden Adlandır** (`Dosya (1).ext`)
- **Yalnızca Eski Dosyaların Üzerine Yaz (Overwrite Older)**
- **Boyutu Farklıysa Değiştir (Replace If Size Different)**
- **Yalnızca Daha Küçük Dosyaları Değiştir (Replace If Smaller)**

---

### 4. Windows Gezgini & Üçüncü Parti Entegrasyonu
- **Windows 11 Modern Sağ-Tık Menüsü**: Sparse Package (`.appx`) ve `IExplorerCommand` ile en üst seviyede yerleşim.
- **Windows Shell Copy Handler**: Standart Ctrl+C / Ctrl+V ve Drag & Drop eylemlerini yakalama (Scroll Lock tuşu ile anında standart Gezgin kopyalamasına dönüş).
- **Total Commander & Directory Opus & XYplorer**:
  ```bash
  deepnode-copy copy *"<fileListPath>" "<targetDir>"
  deepnode-copy move *"<fileListPath>" "<targetDir>"
  deepnode-copy verify "<filePath>" --expected <hash>
  ```

---

### 5. Arayüz (GUI) & Kullanıcı Deneyimi
- **Kompakt (Mini İlerleme Barı)**: TeraCopy tarzı ultra minimalist ilerleme penceresi.
- **Genişletilmiş Görünüm**: Gerçek zamanlı dalga formu Hız Grafiği (Speed Chart), detaylı dosya listesi, Başarılı/Atlanan/Hatalı filtreleri.
- **Her Zaman Üstte (Always On Top)** desteği.
- **Bittiğinde Yapılacak Eylemler (On Finish)**:
  - Bilgisayarı Kapat (Shutdown)
  - Uyku Modu (Sleep) / Hazırda Beklet (Hibernate)
  - Uygulamayı Kapat
  - Tamamlanma & Hata WAV Ses Bildirimleri.
- **Çoklu Dil (i18n)**: Türkçe ve İngilizce desteği.

---

## 🛠️ Proje Modülleri

```
DeepNode Copy/
├── Cargo.toml                  # Rust Workspace kök dosyası
├── crates/
│   ├── deepnode-core/          # Win32 Asenkron Direct I/O, Hash, Engine, Queue, VSS, Conflict
│   ├── deepnode-db/            # SQLite oturum logları ve transfer geçmişi
│   ├── deepnode-shell/         # Win11 Sparse Package, COM IExplorerCommand, Copy Handler Hook
│   ├── deepnode-cli/           # Total Commander, Opus, CLI ve otomasyon aracı
│   └── deepnode-gui/src-tauri/ # Tauri v2 desktop ana uygulaması
├── src/                        # React + TypeScript + Tailwind CSS arayüzü
├── package.json                # Vite & React yapılandırması
├── Total Commander.txt         # Total Commander buton entegrasyon ayarı
├── Directory Opus.txt          # Directory Opus XML komut entegrasyonu
└── XYplorer.txt                # XYplorer script entegrasyonu
```

---

## 🚀 Derleme ve Çalıştırma

### CLI Çalıştırma:
```powershell
cargo run -p deepnode-cli -- copy "C:\Kaynak\Dosya.mkv" "D:\Hedef"
cargo run -p deepnode-cli -- verify "D:\Hedef\Dosya.mkv" --expected 7f83b1...
```

### GUI Geliştirme Modu:
```powershell
npm run dev
cargo tauri dev
```
