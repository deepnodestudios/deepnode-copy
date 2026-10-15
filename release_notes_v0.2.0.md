## DeepNode Copy v0.2.0

Yüksek başarımlı, donanım hızlandırmalı Windows dosya kopyalama, taşıma ve doğrulama aracı DeepNode Copy'nin v0.2.0 sürümü yayınlandı.

---

### Yenilikler ve Öne Çıkan Özellikler (What's New)

- **Klasör Ağacı ve Dinamik Genişletme/Daraltma (+ / -) Desteği:**
  - Sürüklenen veya listeye eklenen klasörler liste üzerinde hiyerarşik grup başlığı olarak gösterilir.
  - Klasör başlıklarının soluna yerleştirilen `+` / `-` butonları ile klasör içeriği anında genişletilebilir veya daraltılabilir.
  - Klasör altındaki dosyalar görsel olarak girintilenerek düzenli bir ağaç görünümü sağlanır.
  - Klasör onay kutusu üzerinden altındaki tüm dosyalar tek tıkla toplu olarak seçilebilir/seçimi kaldırılabilir (Indeterminate durum desteği).

- **Her Dosya ve Klasör İçin Canlı İlerleme Çubuğu ve Yüzde Göstergesi:**
  - Tablonun sağ sütununda hem ana klasörler hem de bireysel dosyalar için yatay mini ilerleme çubuğu ve monospace yüzde etiketi eklendi.
  - Aktif aktarılan dosyalarda bayt hassasiyetinde canlı akan elektrik mavisi/camgöbeği gradient çubuğu ve `%XX` gösterimi.
  - Tamamlanan dosya ve klasörlerde `%100` yeşil onay çubuğu ve kontrol ikonu.
  - Kuyrukta bekleyen, atlanan ve hata alan dosyalar için net renk ve durum göstergeleri.

- **Yerel Windows Gezgini Sürükle-Bırak (Native Drag-and-Drop) Entegrasyonu:**
  - Windows Gezgini'nden pencereye bırakılan klasör ve dosyalar yerel işletim sistemi olayları üzerinden anında algılanır.
  - Hedef klasör giriş kutusuna bırakılan dizinler otomatik olarak hedef konum olarak atanır.
  - Dosya ve klasörler eklendiği anda arka planda derin önizleme taraması yapılarak toplam boyut ve dosya sayısı anında senkronize edilir.

- **Windows Sağ Tık Menü Entegrasyonu (Windows 10 ve Windows 11):**
  - Ayarlar sekmesinden tek tıkla Windows Gezgini sağ tık menüsüne ekleme ve kaldırma olanağı.
  - Dosyalara, klasörlere ve sürücülere sağ tıklandığında "DeepNode Copy ile Kopyala" ve "DeepNode Copy ile Taşı" seçenekleri.
  - Klasör arka planında sağ tıklandığında "DeepNode Copy ile Buraya Yapıştır" desteği.
  - Anlık durum göstergesi (Etkin / Kayıtlı Değil) ve tek tıkla menüyü güncelleme / kaldırma yönetimi.

- **Kriptografik Bütünlük ve SIMD Doğrulama Motoru:**
  - AVX2 ve AVX-512 donanım hızlandırmalı SIMD xxHash3 (30 GB/s), BLAKE3, SHA-256 ve MD5 sağlama algoritmaları.
  - Aktarım sonrasında otomatik ikili sağlama ve doğrulama raporlaması.

- **Akıllı Çakışma Yönetimi:**
  - Mevcut hedef dosyalar için kullanıcıya özel modern çakışma çözme modalı: Otomatik Yeniden Adlandır, Üzerine Yaz, Yalnızca Daha Yeniyse Yaz, Atla.

- **Performans ve Sıfır Donma (Zero-Lag UI):**
  - Yüksek hızlı NVMe/SSD aktarımlarında arayüzün akıcı kalması için re-render optimizasyonları ve memoize satır mimarisi.
  - Aktarım sırasında sağ tık menüsü açıldığında anında tepki veren olay yakalama sistemi.

- **Gelişmiş Çoklu Dil Desteği:**
  - Türkçe, İngilizce, Almanca, Fransızca, İspanyolca, İtalyanca, Portekizce, Rusça, Çince, Japonca, Korece, Arapça ve diğer 8 dille birlikte toplam 20 dilde tam yerelleştirme.

---

### İndirilebilir Dosyalar (Assets)
- **DeepNodeCopy_Setup.exe** — Windows x64 NSIS Kurulum Paketi (Önerilen)
- **DeepNodeCopy_Setup.msi** — Windows x64 WiX MSI Kurulum Paketi
- **DeepNodeCopy.exe** — Taşınabilir (Portable) Bağımsız Yürütülebilir Dosya
