## DeepNode Copy v0.3.6

### Öne Çıkanlar
- **%100'den sonraki bekleme artık görünür:** Kopyalama %100'e ulaştıktan sonra dosyanın diske yazılması ve doğrulanması sürerken pencere sessizce bekliyordu. Artık hız göstergesinin yerinde **"Diske yazılıyor…"** ve **"Doğrulanıyor %X"** yazıyor; ne yapıldığı ve ne kadar kaldığı görülüyor.

### Düzeltmeler
- İlerleme çubuğu, diske yazma başlamadan önce dosyanın gerçek sonuna ulaşır.
- Diske yazma sırasında verilen iptal artık dikkate alınır; yarım kalan geçici dosya temizlenir.
- Geçmiş kayıtlarına doğrulama özetleri (hash) ve hata mesajları yazılır; daha önce bu alanlar boş kalıyordu.
- Tamamlanan aktarımlarda bir dosyanın geçmişte "bekliyor" olarak görünmesine yol açan zamanlama hatası giderildi.
- Büyük dosyalarda doğrulama okuması daha büyük bloklarla yapılır.

---

### İndirilebilir Dosyalar
- **DeepNodeCopy_Setup_x64_v0.3.6.exe** — Windows x64 NSIS kurulum paketi (önerilen)
- **DeepNodeCopy_Setup_x64_v0.3.6.msi** — Windows x64 MSI kurulum paketi
- **deepnode-gui_v0.3.6.exe** — Taşınabilir (portable) sürüm
