## DeepNode Copy v0.2.2

DeepNode Copy v0.2.2 sürümü; otomatik güncelleme altyapısının kararlılığı, antivirüs / AMSI sezgisel uyarılarının önlenmesi ve geçmiş görünümündeki arayüz çakışmalarının giderilmesi amacıyla yayınlandı.

---

### Yenilikler ve Hata Düzeltmeleri (What's New & Fixes)

#### 1. Otomatik Güncelleme ve Ağ İletişimi (Update System)
- **Doğrudan Rust Arka Planından Güncelleme Denetimi:** Arayüzün JavaScript `fetch` motoru üzerinden `api.github.com` adresine yaptığı çağrılarda oluşan `Failed to fetch (api.github.com)` hatası giderildi. Güncelleme denetimi doğrudan Tauri Rust arka planındaki yerel `check_latest_release` (`reqwest`) IPC komutuna taşındı.
- **CSP (Content Security Policy) Genişletildi:** `tauri.conf.json` içerisindeki `connect-src` ilkesine `https://api.github.com`, `https://github.com` ve `https://objects.githubusercontent.com` adresleri eklenerek ağ güvenliği standartlaştırıldı.
- **Doğrulanmış İmzalı Yükleyici Önceliği:** Güncelleme denetleyicisinde öncelikli olarak dijital imzalı `DeepNodeCopy_Setup.exe` kurulum paketi tercih edilir.

#### 2. Antivirüs Sezgisel (Heuristic / AMSI) Koruması
- **Yerel Win32 `ShellExecuteW` Entegrasyonu:** Güncelleme indirildikten sonra arka planda gizli (`CREATE_NO_WINDOW`) bir PowerShell süreci çalıştırma mantığı tamamen kaldırıldı. Yerine Windows kabuğunun yerel `ShellExecuteW` (`"runas"`) API'si entegre edildi.
- **AMSI Sezgisel Taraması Engellendi:** Arka planda hiçbir PowerShell süreci tetiklenmediği için antivirüslerin script tabanlı zararlı yazılım (dropper) uyarıları vermesi engellendi; resmi Windows UAC onay penceresi doğrudan açılır.

#### 3. Aktarım Geçmişi Arayüzü (History View)
- **Tarih ve Silme Butonu Çakışması Giderildi:** Geçmiş kartlarının sağ üst köşesindeki silme butonu mutlak (`absolute`) konumlandırmadan çıkarılıp tarih/saat damgasının hemen sağına flex kardeş elemanı olarak hizalandı. Fare kartın üzerine geldiğinde zaman damgası ve çöp kutusu ikonunun üst üste binmesi kalıcı olarak çözüldü.

#### 4. Paketleme ve İmzalama
- Dağıtımdaki tüm yürütülebilir dosyalar, kurulum paketleri (`.exe` ve `.msi`) ve kabuk kütüphaneleri Authenticode dijital güvenlik sertifikası ile imzalandı.

---

### İndirilebilir Dosyalar (Assets)
- **DeepNodeCopy_Setup.exe** — Windows x64 NSIS Kurulum Paketi (Önerilen)
- **DeepNodeCopy_Setup.msi** — Windows x64 WiX MSI Kurulum Paketi
- **DeepNodeCopy.exe** — Taşınabilir (Portable) Bağımsız Yürütülebilir Dosya
- **DeepNodeCopy.msix** — Windows 11 Modern Kabuk Entegrasyon Paketi
