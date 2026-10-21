## DeepNode Copy v0.3.4

### Düzeltmeler
- **Sürükle-bırak ve kes/yapıştır aktarımlarındaki "Win32 error (code 123)" hatası giderildi:** Kabuk uzantısı, seçilen dosyaları geçici bir liste dosyasıyla birlikte hedef klasöre gönderiyordu. Uygulama bu iki argümanı tek bir geçersiz yolda birleştiriyor (`...\deepnode_list_*.tmp D:\`) ve tüm dosyalar hata veriyordu. Artık liste dosyası ile hedef klasör ayrı işleniyor; aynı kaynaktan farklı bir klasöre taşıma/kopyalama sorunsuz çalışıyor.
- Boşluk içeren dosya yollarının komut satırında bölünmesi durumunda doğru birleştirilmesi korundu.

---

### İndirilebilir Dosyalar
- **DeepNodeCopy_Setup_x64_v0.3.4.exe** — Windows x64 NSIS kurulum paketi (önerilen)
- **DeepNodeCopy_Setup_x64_v0.3.4.msi** — Windows x64 MSI kurulum paketi
- **deepnode-gui_v0.3.4.exe** — Taşınabilir (portable) sürüm
