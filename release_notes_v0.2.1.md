## DeepNode Copy v0.2.1

DeepNode Copy v0.2.1 sürümü; çekirdek I/O motoru kararlılığı, Windows Gezgini (Explorer) kabuk entegrasyonu, veritabanı güvenliği ve ön uç arayüz performansına yönelik kapsamlı mimari iyileştirmeler ve hata düzeltmeleriyle yayınlandı.

---

### Yenilikler ve Hata Düzeltmeleri (What's New & Fixes)

#### 1. Çekirdek Aktarım Motoru & Direct I/O (Core Engine)
- **Taşıma (Move) Modu Veri Güvenliği:** Taşıma işlemlerinde kaynak dosyanın silinmesi öncesinde hedef dosyanın varlığı, bayt boyutu ve kriptografik sağlama doğrulaması zorunlu kılındı. Doğrulanamayan hiçbir kaynak silinmez.
- **Direct I/O Sektör Hizalama:** Sektör sınırına (512 / 4096 bayt) tam bölünmeyen dosyalarda Direct I/O tampon taşması ve artık bayt (slack bytes) sorunları giderildi.
- **ADS, ACL ve Zaman Damgası Hata Yayılımı:** NTFS Alternate Data Streams, erişim kontrol listeleri (ACL) ve zaman damgası kopyalama hataları sessizce yutulmak yerine teşhis loglarına ve aktarım olay akışına aktarılır.
- **Boş Dizin Kopyalama:** Derin klasör hiyerarşilerinde boş alt dizinlerin hedefe birebir kopyalanması sağlandı.
- **Kuyruk ve İptal Yönetimi:** Dosya kopyalama sırasında kuyruk yöneticisi (QueueManager) görev yürütme sırası optimize edildi; iptal edilen dosyalarda hedefteki yarım kalmış artık dosyalar otomatik olarak temizlenir.

#### 2. Windows Gezgini Kabuk Entegrasyonu (Shell Extension)
- **Explorer.exe Çökme Koruması:** `crates/deepnode-shell` altındaki tüm COM geri çağrıları (`GetTitle`, `GetIcon`, `GetState`, `InvokeCommand`, `QueryContextMenu`, `Initialize`) `std::panic::catch_unwind` ile korunarak kabuk süreçlerinde panik kaynaklı çökme riski tamamen ortadan kaldırıldı.
- **Zehirlenmiş Mutex Kurtarma:** COM nesnelerindeki iç durum kilitleri (lock) zehirlenmeye (poisoning) karşı dirençli hale getirildi.
- **Gerçek COM Kayıt Entegrasyonu:** `DllRegisterServer` ve `DllUnregisterServer` Win32 Registry API'leri (`RegCreateKeyExW`, `RegSetValueExW`, `RegDeleteTreeW`) ile donatıldı; `CLSID_DEEPNODE_COMMAND` ve `CLSID_DEEPNODE_DRAGDROP` için `InprocServer32` ("Apartment") ve `shellex\DragDropHandlers` / `shellex\ContextMenuHandlers` anahtarları eksiksiz kaydediliyor.
- **Windows 11 Sparse Package Desteği:** PowerShell `Add-AppxPackage` üzerinden dış içerikli sparse paket kaydında zorunlu `-DisableDevelopmentMode` bayrağı eklendi.

#### 3. Veritabanı ve Geçmiş Yönetimi (DeepNode DB)
- **Otomatik Saklama Politikası (Retention Policy):** Oturum tamamlanma anında otomatik olarak devreye giren saklama politikası ile eski veya sınır kotasını aşan oturumlar ve ilişkili dosya kayıtları transaction korumasıyla temizlenir.
- **Negatif LIMIT Taşma Koruması:** `usize::MAX` değerinin negatif `LIMIT` parametresine dönüşüp tüm tabloyu belleğe dökmesini engelleyen güvenli sayısal sınırlandırma uygulandı.

#### 4. Kullanıcı Deneyimi & Ön Uç (Frontend UI)
- **Çoklu Seçim Kaldırma Yarışı Giderildi:** Kuyruktan toplu dosya kaldırma işleminde state ezilmesi ve gereksiz önizleme taramaları çözüldü.
- **Başlangıç Dili ve HTML Senkronizasyonu:** `index.html` başlangıç kökü `<html lang="tr" class="dark">` olarak güncellendi ve `localStorage` dil tercihi React yüklenmeden önce DOM'a anında yansıtıldı.
- **Sürüm Eşitlemesi:** CLI, Tauri backend ve arayüz sürüm bilgileri dinamik olarak senkronize edildi.

---

### İndirilebilir Dosyalar (Assets)
- **DeepNodeCopy_Setup.exe** — Windows x64 NSIS Kurulum Paketi (Önerilen)
- **DeepNodeCopy_Setup.msi** — Windows x64 WiX MSI Kurulum Paketi
- **DeepNodeCopy.exe** — Taşınabilir (Portable) Bağımsız Yürütülebilir Dosya
- **DeepNodeCopy.msix** — Windows 11 Modern Kabuk Paketi
