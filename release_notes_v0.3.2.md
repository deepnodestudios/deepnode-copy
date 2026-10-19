## DeepNode Copy v0.3.2

### Düzeltmeler
- **Windows başlangıcında otomatik açılma kaldırıldı:** Uygulama artık bilgisayar açılışında başlamaz. Kopyalama ve taşıma işlemlerinde kabuk uzantısı uygulamayı gerektiğinde kendisi açar (TeraCopy gibi). Önceki sürümlerin eklediği başlangıç kaydı güncelleme sırasında otomatik olarak silinir.
- **Ana pencere artık her zaman en önde kalmıyor:** Yalnızca seçim penceresi en üstte açılır. Aktarım ve ana pencere, diğer pencerelerin arkasına normal şekilde geçebilir; "her zaman üstte" düğmesi tekrar kullanıcının seçimine göre çalışır.
- Arka plan (`--silent`) modunda başlatılan uygulama artık pencere göstermez.

Sessiz güncelleme (silent update) değişmedi: uygulama her açılışta yeni sürümü denetler ve kapanışta sessizce kurar.

---

### İndirilebilir Dosyalar
- **DeepNodeCopy_Setup_x64.exe** — Windows x64 NSIS kurulum paketi (önerilen)
- **DeepNodeCopy_Setup_x64.msi** — Windows x64 MSI kurulum paketi
- **deepnode-gui.exe** — Taşınabilir (portable) sürüm
