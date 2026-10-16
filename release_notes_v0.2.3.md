## DeepNode Copy v0.2.3

DeepNode Copy v0.2.3 sürümü; taşınabilir (portable) ve yerel kurulum güncellemelerinde yaşanan `cmd.exe` tırnak ayrıştırma (Windows `\\` bulunamıyor) hatasını gidermek ve otomatik güncelleme sürecini bütünüyle yerel Win32 API'leri ile güçlendirmek amacıyla yayınlandı.

---

### Yenilikler ve Hata Düzeltmeleri (What's New & Fixes)

#### 1. Yerinde (In-Process) Atomik Güncelleme ve Takas (Atomic Update Swap)
- **Windows `\\` Bulunamıyor Hatası Giderildi:** Taşınabilir veya yerel güncellemeler sırasında arka planda çağrılan `cmd.exe /C start ""` komutunun Windows CRT kaçış karakterleri (`\"\"`) ile çakışması sonucu ortaya çıkan `Windows '\\' öğesini bulamıyor` uyarısı kalıcı olarak ortadan kaldırıldı.
- **Doğrudan Win32 Süreç Değişimi:** Çalışan yürütülebilir dosya (`.exe`) açıkken NTFS üzerinde anında `.exe.old` olarak yeniden adlandırılır, indirilen güncel sürüm orijinal konuma atomik olarak kopyalanır ve `ShellExecuteW` ile doğrudan yeni sürüm başlatılır.
- **Artık Dosya Temizliği:** Uygulama bir sonraki açılışında önceki güncellemeden kalan `.old`, `.part` veya geçici dosyaları otomatik olarak temizler.

#### 2. Kurulum ve Taşınabilir (Portable) Mod Tespiti
- `AppData\Local\DeepNode Copy` veya `Program Files` dizinlerinde kurulu olan uygulamalar artık `uninstall.exe` varlığı kontrol edilerek kesin olarak kurulum modu olarak sınıflandırılır.
- Kurulu istemciler doğrudan doğrulanmış ve dijital imzalı `DeepNodeCopy_Setup.exe` paketini çalıştırarak temiz bir güncelleme deneyimi yaşar.
- Taşınabilir kullanım için bağımsız arşiv paketi (`DeepNodeCopy_Portable.zip`) sunulmuştur.

#### 3. Dijital İmza ve Güvenlik
- Dağıtımdaki tüm kurulum dosyaları (`.exe`, `.msi`) ve modern kabuk entegrasyon paketleri (`.msix`) Authenticode SHA256 sertifikası ile imzalanmıştır.

---

### İndirilebilir Dosyalar (Assets)
- **DeepNodeCopy_Setup.exe** — Windows x64 NSIS Kurulum Paketi (Önerilen)
- **DeepNodeCopy_Setup.msi** — Windows x64 WiX MSI Kurulum Paketi
- **DeepNodeCopy_Portable.zip** — Taşınabilir (Portable) Bağımsız Sürüm
- **DeepNodeCopy.msix** — Windows 11 Modern Kabuk Entegrasyon Paketi
