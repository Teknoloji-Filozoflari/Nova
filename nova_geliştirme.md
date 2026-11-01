# NOVA — Faz 6: Fare, Ayarlar, Mağaza ve Dosya Açma Deneyimi

Nova'nın Global Project Instructions kurallarını ve önceki fazlarda alınan mimari kararları uygula.

## Amaç

Nova'yı mevcut terminal uygulaması kataloğunu koruyarak daha kolay kişiselleştirilebilir hâle getir. Fareyle seçim, kısa kategori adları, kullanışlı araç detayları, ayarlar, güvenilir bir uygulama mağazası ve temel dosya seçme akışı ekle.

TUIUI'yi yalnızca etkileşim ve kategori fikri için referans al. Ekran tasarımını, kodunu veya uygulama listesini kopyalama. Nova içine sohbet asistanı paneli ekleme; kullanıcı ayarlardan varsayılan AI komut satırı uygulamasını seçmek istiyor.

## Önce yapılacak inceleme

1. Mevcut depo yapısını, README.md, ARCHITECTURE.md, Cargo.toml, katalog ve kullanıcı yapılandırmasını, uygulama durumunu, olay döngüsünü, terminal korumasını, başlatıcıyı ve testleri incele.
2. Mevcut çalışan özellikleri ve kullanıcı verilerini koru. Gerekmedikçe yeni bağımlılık ekleme.
3. Bu kapsamı ve değişiklik sırasını kısa bir planla açıkla.
4. Mevcut mimariye uymayan veya güvenli biçimde gerçekleştirilemeyen bir istek varsa nedenini belirt ve güvenli alternatifi uygula.
5. Global talimattaki gibi yalnızca bu faz üzerinde çalış; tamamlanınca dur ve sonraki faz onayını bekle.

## 1. Fare ve kısa kategori adları

- Klavye kullanımını aynen koru; fareyle kategori ve araç seçimi ekle.
- Kategori ve araç listelerinde tıklama ve kaydırma çalışsın. Yardım, ayarlar ve onay pencerelerinde fare davranışı varsa tutarlı olsun.
- Terminal boyutu değişince tıklama alanları güncel düzene göre hesaplanmalı.
- Crossterm fare yakalamasını normal çıkışta, hata ve panik yollarında güvenli biçimde kapat; terminalin fare/ekran durumunu geri yükle.
- Kategori kimliklerini ve yapılandırmada kullanılan mevcut değerleri değiştirme. Yalnızca arayüzde şu kısa adları göster:
  - AI
  - Git
  - Dosya
  - Sistem
  - Ağ
  - Docker
  - Yardımcılar
  - Özel
- Arama kısa görünen adlarla ve mevcut kategori adlarıyla da çalışsın.
- Dar terminal düzeninde fare ve klavye ile erişim bozulmasın.

## 2. Araç detay alanı

Seçili araç için kullanılabilir alanı değerlendirip şunları göster:

- Kısa ve anlaşılır açıklama
- Kurulu olma durumu
- Çalıştırılacak komut
- Güvenli ve doğrulanmış kullanım örnekleri veya kısayollar
- Gerekliyse kabuk entegrasyonu ya da ek kurulum bilgisi

Kullanım örneklerini katalog metaverisi olarak tut. Doğrulanmamış komut veya özellik uydurma. Örnekleri kullanıcı açıkça başlatmadıkça çalıştırma. Küçük terminallerde metni kırp veya uygun biçimde yer değiştir; taşma ve panik oluşturma.

## 3. Ayarlar ve varsayılan uygulamalar

Var olan ayar ekranını genişlet; paralel ve gereksiz bir ayar sistemi kurma.

Kullanıcı şunları yapabilsin:

- Tema seçip anında önizleme ve uygulama
- Özel araçları ekleme, düzenleme ve silme
- Kurulu AI CLI araçlarından varsayılan olanı seçme
- Resim ve video için varsayılan uygulamayı seçme
- Mevcut terminal ve çalışma dizini ayarlarını koruma

Varsayılan AI seçimi yalnızca başlatılabilir araçları sunmalı. Nova içinde sohbet paneli veya model API entegrasyonu yapma.

Ayarları mevcut kullanıcı yapılandırması/state yaklaşımına uygun şekilde kalıcı sakla. Bozuk yapılandırmayı sessizce silme veya üzerine yazma. Silme ve önemli değişikliklerde onay iste.

### MiniMax Code

- Gömülü kataloğa MiniMax Code kaydı ekle: görünen ad “MiniMax Code”, executable `mcode`.
- PATH kontrolüne ek olarak Linux için belgelenmiş varsayılan kullanıcı yolu `~/.minimax-code/bin/mcode` konumunu kontrol et.
- Kullanıcıya `mcode` PATH'te bulunamadığında, bulunan yolu veya düzeltme/özel araç ekleme yolunu anlaşılır biçimde göster.
- Uygulamayı tespit etmek için çalıştırma veya `--version` çağırma. PATH ya da bilinen kurulum yolunda dosya araması yeterlidir.
- Kullanıcı özel executable yolu belirleyebilsin. Mutlak yol açıkça girilmiş/seçilmişse dosyanın varlığını, çalıştırılabilirliğini ve yol türünü doğrula; kabuk kullanma.

## 4. Mağaza

Mağaza kataloğunu uygulamanın mevcut araç kataloğundan ayrı tut. Mağaza kaydı en az şunları taşısın:

- Değişmez kimlik
- Uygulama adı ve Türkçe kısa açıklama
- Kategori
- Kaynak/proje sayfası ve lisans bilgisi
- Executable adı
- Kurulum yöntemi ve desteklenen platform bilgisi
- Varsa doğrulama özeti/checksum bilgisi

Kullanıcı uygulamaları kategoriye ve ada göre arayıp inceleyebilsin. Mağaza detayında uygulamanın ne işe yaradığı, kaynağı, lisansı, kurulu olup olmadığı ve desteklenen kurulum yöntemi gösterilsin.

Kategorileri keşfetmeyi kolaylaştıracak biçimde düzenle; örneğin AI, Git, dosya ve arama, sistem, ağ, konteyner, veritabanı, güvenlik, medya ve günlük araçlar. Kategori isimleri kısa ve anlaşılır olsun.

TUIUI'nin 636 uygulamalık listesini kopyalama veya sayı hedefi uğruna doğrulanmamış kayıt ekleme. Bunun yerine bakımı yapılabilir, kaynağı ve lisansı doğrulanmış genişletilebilir bir başlangıç kataloğu oluştur. Her kayıt için upstream bilgilerini kontrol et. Katalog verisini UI koduna gömme.

### Kurulum güvenliği

- Kullanıcı bir uygulamayı seçtiğinde kurulumdan önce uygulama, sürüm, kaynak, hedef dizin ve yapılacak işlemleri göster; açık onay al.
- Kurulum yalnızca kullanıcı alanına yapılmalı. `sudo`, root yetkisi, sistem dizinlerine yazma veya sistem paket yöneticisiyle kurulum yok.
- Kurulum kaynak/hedefleri kullanıcıya ait Nova dizinleri veya uygun kullanıcı dizinleriyle sınırlı olsun.
- Varsayılan olarak uzaktan indirilen betikleri shell üzerinden çalıştırma (`curl | sh` vb. yok).
- Programı ve argümanlarını ayrı geçir; shell komutu birleştirme. Arşiv açılıyorsa yol geçişi ve dosya üzerine yazma saldırılarını engelle.
- İndirme için upstream checksum/imza sağlıyorsa doğrula. Doğrulanabilir veya güvenli kullanıcı alanı yöntemi olmayan uygulamalarda “Kur” eylemi verme; resmî kurulum yönergelerini göster.
- Her kurulum tek uygulama için ve kullanıcı onayıyla yürüsün. Başarı mesajı yalnızca kurulumun doğrulanabilen sonucunu anlatsın.
- Ağ, indirme, izin ve bozuk arşiv hatalarını anlaşılır biçimde göster.

## 5. Dosya seçici ve medya uygulamaları

Nova'ya tam kapsamlı dosya yöneticisi ekleme. Kullanıcıya terminal içinde temel bir dosya seçici sun:

- Dizinler arasında gezinme ve dosya seçme
- Klavye ve fare kullanabilme
- Seçilen resim veya videoyu ayarlarda belirlenmiş uygulamayla açma
- Dosya türüyle varsayılan uygulama uyumsuzsa anlaşılır hata gösterme

Dosya silme, taşıma veya yeniden adlandırma ekleme. Açma işlemini shell kullanmadan, executable ve dosya yolunu ayrı argümanlar olarak başlat. Terminal dışı medya uygulamalarını ayrı ve küçük bir başlatma yolu olarak ele al; Nova'nın Konsole sekme başlatıcısını bozma. Sadece sürecin başlatılmasını doğrulayabiliyorsan dosyanın başarıyla açıldığını iddia etme.

## 6. Nova güncellemesi

Ayarlar içinde kullanıcı tarafından başlatılan “Güncellemeleri denetle” eylemi ekle.

- Önce GitHub deposundaki gerçek yayın/release düzenini ve mevcut dağıtım biçimini incele.
- Resmî bir sürüm kaynağı veya güvenilir güncelleme paketi yoksa otomatik güncelleme varmış gibi gösterme; mevcut sürümü ve kullanıcıya gereken adımları açıkla.
- Güncelleme varsa mevcut ve yeni sürümü, yayın notlarını, kaynağı ve doğrulama bilgisini göster.
- İndirme/kurulumdan önce ayrıca açık onay al.
- Güncellemeyi kullanıcı alanında yap; sudo/root kullanma ve kullanıcı yapılandırmasına dokunma.
- İndirme veya doğrulama başarısızsa çalışan Nova ikilisini bozma.
- Güncelleyiciyi shell betiği ya da belirsiz bir Git komutu çalıştırarak uygulama.

## 7. Testler ve belgeler

Global proje talimatlarına göre ilgili formatlama, derleme, test ve lint kontrollerini çalıştır; sonuçları gerçekten çalıştırıldıkları ölçüde raporla.

En az şunları kapsayan testler ekle veya güncelle:

- Fare tıklama, kaydırma ve resize koordinatları
- Kısa kategori adları ile mevcut kategori kimliklerinin eşleşmesi
- Tema ve varsayılan uygulama ayarlarının kalıcılığı
- PATH dışındaki açık executable yolu ve `mcode` algılama
- Mağaza kaydı doğrulama, arama ve kurulum onayı
- Shell çalıştırılmadığının ve sudo/system dizinlerine yazılmadığının kontrolü
- İndirme/checksum/arşiv hata senaryoları
- Dosya seçici ve medya başlatıcısının ayrık süreç oluşturması
- Güncelleme yok, sürüm mevcut, doğrulama başarısız ve kullanıcı reddi senaryoları
- Küçük terminal düzeni ve terminal durumunun geri yüklenmesi

Testler gerçek uygulama, gerçek terminal sekmesi veya gerçek kullanıcı dosyası gerektirmesin; uygun yerlerde sahte filesystem, downloader, launcher ve update provider kullan.

README.md ve ARCHITECTURE.md'yi yeni ayar, kategori, mağaza, kurulum/güncelleme güvenliği ve dosya seçici davranışıyla tutarlı biçimde güncelle.

## Kabul kriterleri

- Fare ve klavye ile araç/kategori seçilebiliyor; terminal durumu çıkışta geri yükleniyor.
- Kısa kategori adları görünürken mevcut kategori kimlikleri ve kullanıcı yapılandırması korunuyor.
- MiniMax Code `mcode` olarak görünüyor; PATH dışında standart kullanıcı konumunda kuruluysa tespit edilebiliyor.
- Tema ve varsayılan uygulama seçimleri yeniden başlatma sonrasında korunuyor.
- Mağaza güvenilir kaynaklı uygulamaları gösteriyor; kurulum yalnızca açık onayla ve kullanıcı alanına yapılıyor.
- Dosya seçici seçilen dosyayı yapılandırılmış medya uygulamasına shell kullanmadan aktarabiliyor.
- Nova güncellemesi sadece gerçek ve doğrulanmış bir yayın yöntemi varsa sunuluyor; kullanıcı onayı olmadan güncelleme yapmıyor.
- Mevcut başlatıcı, katalog, favoriler ve kullanıcı verileri korunuyor.
- Kontrol sonuçları, değişen dosyalar, bilinen eksikler ve sonraki faz önerisi raporlanıyor.

Faz sonunda dur ve Faz 7 için onayımı bekle.
