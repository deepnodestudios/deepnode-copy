## DeepNode Copy v0.2.4

DeepNode Copy v0.2.4 sürümü; endüstri standardı dosya kopyalama araçlarının mimarisine uygun olarak **arka planda tamamen sessiz güncelleme (silent background update)** yeteneğini getirmektedir.

---

### Yenilikler ve Mimari İyileştirmeler (What's New)

#### 1. Arka Planda Sessiz Güncelleme Hizmeti (Silent Background Service)
- **Kullanıcıyı Asla Bölmeyen Arka Plan İndirmesi:** Uygulama açıldıktan 30 saniye sonra, aktif bir dosya aktarımı olmadığı doğrulanarak GitHub üzerinden yeni sürüm denetlenir. Yeni bir sürüm varsa düşük öncelikli olarak arka planda sessizce indirilir ve doğrulanır (`%APPDATA%\DeepNodeCopy\updates\`).
- **Aktif Aktarıma Tam Öncelik:** Arka plan güncelleme servisi, devam eden kopyalama/taşıma işlemlerinin bant genişliğini ve disk I/O performansını asla etkilemez; aktarım tamamlanana kadar güncellemeyi bekletir.
- **Kriptografik Doğrulama:** İndirilen paket SHA256 Authenticode dijital güvenlik sertifikası kontrolünden geçirilir ve hazır bekleyen (`staged`) güncelleme olarak işaretlenir.

#### 2. Ekranda İstenmeyen Pencere Açılmasını Önleme
- **Sıfır İstenmeyen Pencere Politikası:** Bilgisayar açılışında veya güncelleme tamamlandığında ekranda hiçbir zaman boş ya da kendiliğinden beliren bir pencere açılmaz.
- **Kapanış Sırasında Sessiz Kurulum (On-Exit):** Kullanıcı kopyalama/taşıma işlemini tamamlayıp pencereyi kapattığında, hazır bekleyen güncelleme arka planda tamamen sessizce (`/S`) kurulur ve tamamlanır; masaüstünde hiçbir bildirim ya da pencere açılmaz.
- **Sonraki Kullanımda Hazır Başlama:** Kullanıcı gelecekte herhangi bir zamanda sağ tıklayıp bir dosya kopyaladığında veya taşıdığında, uygulama doğrudan güncellenmiş yeni sürümüyle açılarak istenen işlemi anında yerine getirir.

#### 3. "Hakkında" Penceresi Durum Kartı
- Kullanıcı dilediğinde "Hakkında" penceresini açarak arka planda hazır bekleyen güncellemeyi görebilir ve isterse "Şimdi Yeniden Başlat ve Güncelle" butonuna basarak anında uygulayabilir.

---

### İndirilebilir Dosyalar (Assets)
- **DeepNodeCopy_Setup.exe** — Windows x64 NSIS Kurulum Paketi (Önerilen)
- **DeepNodeCopy_Setup.msi** — Windows x64 WiX MSI Kurulum Paketi
- **DeepNodeCopy_Portable.zip** — Taşınabilir (Portable) Bağımsız Sürüm
- **DeepNodeCopy.msix** — Windows 11 Modern Kabuk Entegrasyon Paketi
