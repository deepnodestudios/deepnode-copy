## DeepNode Copy v0.3.5

### Öne Çıkanlar
- **DDM ile aynı imza:** Tüm çıktılar (portable exe, shell DLL, MSIX, NSIS ve MSI kurulumları) DeepNode Download Manager ile aynı `CN=DeepNode Studios` sertifikasıyla imzalanır. Güncelleme doğrulaması da bu sertifikaya sabitlendi.
- **İmzalı Windows 11 modern sağ tık menüsü:** Kabuk entegrasyonu artık imzalı sparse MSIX ile kurulur; geliştirici modu gerekmez. Eski imzalı paket otomatik kaldırılıp yenisi kaydedilir.
- **Kalıcı aktarım ayarları:** Çakışma politikası, doğrulama modu, hash algoritması, Direct I/O ve bitişte yapılacak eylem sonraki pencerelerde korunur.

### Düzeltmeler
- İptalde hedefte zaten var olan veya tamamlanmış dosyalar silinmez; yalnızca geçici `.dnpart` dosyası ve boş kalan oluşturulmuş klasörler temizlenir.
- Atlanan veya kuyruktan kaldırılan dosyalar ilerleme toplamından düşülür; ilerleme artık %100'de kalır.
- Kuyruk eşzamanlılık sınırı uygulanır; tüm görevler birden başlatılmaz.
- Aynı kaynak/hedef seçimi ve hedefin kaynak klasörün içinde olması erken ve anlaşılır hatayla engellenir.
- CopyHook: klasör kopyalamada hedef adının ikinci kez iç içe oluşması giderildi; çoklu kaynak ve tırnaklı yollar desteklendi; tek sürücüdeki klasör taşımaları Explorer'a bırakıldı.
- Seçim penceresi yalnızca **Explorer** ve **DeepNode Copy** seçeneklerini gösterir; Explorer seçildiğinde CopyHook tekrar araya girmez (çift pencere/çift işlem yok).
- Çakışma penceresindeki "tümüne uygula" seçimi her dosyada sıfırlanır.
- VSS: oluşturulan geçici gölge kopyalar aktarım sonunda silinir, mevcut kopyalar korunur; kilitli dosya çözümlemesi sertleştirildi.
- Direct I/O ile yazılan veriler diske flush edilip tam uzunluk doğrulanarak tamamlanır.
- Hatalı tamamlanan aktarımlarda pencere otomatik kapanmaz, "Tekrar Dene" sunulur; kompakt görünümde Direct I/O ve hata durumu gösterilir.
- Genişletilmiş görünümde tamamlanan dosya sayısı ve tepe hız bilgisi eklendi; kuyruk temizleme aktif aktarım sırasında kilitlendi.

---

### İndirilebilir Dosyalar
- **DeepNodeCopy_Setup_x64_v0.3.5.exe** — Windows x64 NSIS kurulum paketi (önerilen)
- **DeepNodeCopy_Setup_x64_v0.3.5.msi** — Windows x64 MSI kurulum paketi
- **deepnode-gui_v0.3.5.exe** — Taşınabilir (portable) sürüm
