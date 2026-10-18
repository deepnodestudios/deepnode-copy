## DeepNode Copy v0.3.0

DeepNode Copy v0.3.0, Windows Gezgini entegrasyonunu TeraCopy'nin mimarisine göre baştan yazar: sürükle-bırak ve Ctrl+V işlemleri artık Gezgin tarafından değil, doğrudan DeepNode Copy seçim penceresi tarafından karşılanır.

---

### Yenilikler ve İyileştirmeler

#### 1. Gezgin Kopyalama Entegrasyonu (TeraCopy tarzı)
- **Sol tuşla sürükle-bırak ve Ctrl+V yakalanıyor:** Kabuk uzantısı `Folder`, `Directory`, `Drive` ve `lnkfile` için DragDropHandler olarak kaydolur ve bırakma menüsünün varsayılan komutunu üstlenir. Gezgin kendi kopyalamasını yapmak yerine DeepNode Copy seçim penceresini açar.
- **Akıllı karar:** Gezgin'in kopyala/taşı varsayılanı izlenir. Aynı sürücü içindeki taşımalar (anında yeniden adlandırma) ve kısayol oluşturma Gezgin'e bırakılır.
- **Sağ tuşla sürüklemede yeni menü öğeleri:** "DeepNode Copy...", "DeepNode Copy here", "DeepNode Move here".
- **Eski hatalı kayıt temizlendi:** Önceki sürümlerin Gezgin'in klasör DropHandler'ını ezen kaydı kaldırılır; bu kayıt yakalamanın hiç çalışmamasına yol açıyordu.
- Scroll Lock açıkken her şey Gezgin'e bırakılır.

#### 2. Seçim Penceresi Konumu
- Pencere, TeraCopy'deki gibi bırakma/yapıştırma anında farenin bulunduğu noktada açılır (sol üst köşe imleçte).
- Farenin bulunduğu monitörün DPI ölçeğine göre boyutlanır; ekran kenarına taşarsa görev çubuğu hariç çalışma alanına kaydırılır. Çoklu monitör desteklenir.

#### 3. Kararlılık ve Güvenlik
- **Siyah pencere sorunu giderildi:** Pencere artık arayüz hazır olduğunda gösterilir; Gezgin'den açılan pencerelerin siyah kalması düzeltildi.
- **Antivirüs uyarıları azaltıldı:** Global klavye/fare kancaları (hook) ve sahte tuş basma kaldırıldı; odak, Gezgin'den yasal yoldan (`AllowSetForegroundWindow`) devralınır.
- **Güncelleme kurulumu düzeltildi:** Kurulum, kabuk uzantısı DLL'ini artık gerçekten günceller.
- Tüm ikililer ve kurulum paketleri DeepNode Studios sertifikasıyla imzalıdır.

---

### İndirilebilir Dosyalar
- **DeepNodeCopy_Setup_x64.exe** — Windows x64 NSIS kurulum paketi (önerilen)
- **DeepNodeCopy_Setup_x64.msi** — Windows x64 MSI kurulum paketi
- **deepnode-gui.exe** — Taşınabilir (portable) sürüm
