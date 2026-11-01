# Faz 4 — Kişiselleştirme: Tasarım ve Uygulama Planı

> Bu belge Faz 4'ün tasarım kararlarını ve adım adım uygulama planını içerir.
> Faz 0–3 tamamlandı; bu fazın sonunda kullanıcıdan Faz 5 onayı beklenecek.

## 1. Problem

Faz 3'e kadar Nova yalnızca **salt okunur** bir katalog gösteriyordu. Favoriler
bellekte tutuluyor, son kullanılanlar kaydedilmiyor, kullanıcı kendi aracını
ekleyemiyor ve hiçbir ayar kalıcı değil. Faz 4 bu boşluğu doldurur:

1. Favoriler kalıcı
2. Son kullanılanlar (sınırlı, temizlenebilir)
3. Özel araç ekleme (doğrulama + gözden geçirme)
4. Favori proje dizinleri (launcher'a taşınır)
5. Klavye tutarlılığı + yıkıcı işlem onayı
6. Ayarlar (tercih edilen terminal, varsayılan çalışma dizini, dosya yolları)

## 2. Tasarım kararları

### 2.1 Durum dosyalarının ayrılması (en önemli karar)

İki **ayrı** dosya kullanılacak. Bu, "bozuk yapılandırma kullanıcı verisini
sessizce kaybettirmemeli" kabul kriterinin temelidir.

| Dosya | Kim yazar | İçerik | Bozuk olursa |
|---|---|---|---|
| `$XDG_CONFIG_HOME/nova/config.toml` (yoksa `~/.config/nova/config.toml`) | Kullanıcı (elle) | `[ui]`, `[[tools]]` | Uygulama açılmaz, dosya:satır hatası |
| `$XDG_CONFIG_HOME/nova/state.toml` (yoksa `~/.config/nova/state.toml`) | Yalnızca Nova | favoriler, geçmiş, proje dizinleri, tercihler | Uygulama açılır, **dosya yedeklenir** (`.bozuk-<zaman>`), boş durumla devam, durum satırında açık uyarı |

Neden ayrı: Nova'nın her favori değişikliğinde elle yazılmış `config.toml`'u
yeniden yazması yorumları/sıralamayı bozar ve tek bir serde alanı yüzünden
tüm kullanıcı araçlarını riske atar. Ayrıca bozuk bir `config.toml` varken
Nova hiç açılmadığı için favori kaydı da yapılamaz; `state.toml` bu kilidi
ÇÖZMEZ ama en azından yarıda kalan başka bir bozulmada veri kaybını önler.

`state.toml` yazımı **atomik** olur: aynı dizinde `.tmp` dosyasına yaz,
`fsync`, sonra `rename`. Yazma başarısız olursa eski dosya dokunulmadan kalır
ve durum satırında hata gösterilir.

### 2.2 Favoriler

- `CatalogEntry::favorite` artık **türetilmiş** değer: yükleme sonunda
  `state.toml`'daki favori **id** listesi uygulanır.
- Favori = **id** kümesi (`BTreeSet<String>`). Görünen ad veya kategori
  değişse de, katalog güncellense de favori korunur.
- Katalogda artık bulunmayan bir id favori listesinde kalır (sitelenmiş
  katalogdan çekilen araç geri gelirse favori geri gelir). `state.toml`
  yazılırken bu id'ler **silinmez**; yalnızca UI'da görünmez.
- Gömülü `assets/catalog/*.toml`'daki `favorite = true` alanı **kaldırılamaz**
  ama artık yalnızca *ilk çalıştırma tohumu* olarak kullanılır: `state.toml`
  yoksa gömülü favoriler kopyalanır; varsa `state.toml` tek otoritedir.
  Böylece kullanıcı bir varsayılan favoriyi kaldırdığında güncelleme onu
  geri getirmez.

### 2.3 Son kullanılanlar (geçmiş)

- Kayıt **yalnızca** `launch()` → `Ok(LaunchOutcome)` döndüğünde yapılır.
  `LaunchOutcome` zaten "emülatör süreci başlatıldı, uygulamanın çalışması
  doğrulanmadı" diyor. Kayıt girişimi, **isteğin gönderildiği** anlamına
  gelir; harici uygulamanın sağlıklı çalıştığı anlamına gelmez.
- Hata durumunda (konsole yok, dizin yok, komut bulunamadı) kayıt yapılmaz.
- Her kayıt: `{ id, name, at: <unix saniye> }`. Aynı id tekrar başlatılırsa
  eski kayıt kaldırılır ve başa eklenir (tekrarlar birleşir).
- Sınır: `[ui] recent_limit` (varsayılan 20, en az 1, en çok 100). Aşınca
  en eski atılır.
- `R` tuşu: geçmiş görünümünü açar. Geçmiş görünümündeyken `X` temizler
  (onay ister). Geçmiş görünümünde bir araç seçmek mümkün değildir; sadece
  görüntüleme + temizleme. Bu, "gereksiz özellik ekleme" ilkesine uyar.
- Zaman biçimlendirme: yeni bağımlılık **yok**. `SystemTime` → epoch saniye;
  yerel saate çevirmek için `libc::localtime_r` gerekir ama `libc` bağımlılığı
  da yok. Bu yüzden ekranda göreli zaman gösterilir: "az önce", "12 dk önce",
  "3 sa önce", "2 gün önce". Bulunulan zaman `App::now_secs` üzerinden
  enjekte edilir; testler belirli zamanları verebilir.

### 2.4 Özel araç ekleme

- `N` tuşu form açar. Alanlar (sırayla): görünen ad, açıklama, kategori,
  çalıştırılabilir, argümanlar, çalışma dizini, çalıştırma türü.
- Kategori ve çalıştırma türü alanları **seçici** (Left/Right veya Space ile
  değişir), serbest metin değil. Böylece geçersiz kategori yazılamaz.
- Argümanlar tek satır olarak alınır ve **boşlukla** ayrılır; kabuk
  kullanılmaz, tırnak/genişletme yok. Form ekranında bu açıkça yazar:
  "Kabuk kullanılmaz; argümanlar boşlukla ayrılır."
- Çalıştırılabilir: yol içeriyorsa (`/` veya `~`) dosya var ve çalıştırılabilir
  olmalı; yol içermiyorsa `PATH`'te aranır (bulunamazsa **uyarı**, engel
  değil — kullanıcı aracı sonradan kurabilir). Boş olamaz.
- Kimlik (`id`): ada göre üretilir (`slugify`), çakışırsa `-2`, `-3`…
  Gömülü bir id ile çakışırsa sayı eklenir.
- **Gözden geçirme adımı**: `Tab`/`Enter` doğrular ve *özet ekranına* geçer
  (ne kaydedileceğinin Tam özet: name, category, executable + args, dir,
  tür). `Enter` = kaydet, `Esc` = forma dön (girdiler korunur).
- Doğrulama hataları form içinde alan altında gösterilir; kayıt engellenir.
  Hatalar başarıdan **renk ve işaret** ile ayrılır (`✗ hata` kırmızı,
  `✓ kaydedildi` yeşil).
- Silme: yalnızca `origin == User` girdiler silinebilir (gömülü araç
  silinemez, yalnızca "gizle" ileride). `D` tuşu onay ekranı açar
  ("<ad> kalıcı olarak silinsin mi? Enter onaylar, Esc vazgeçer").
  Silme `config.toml`'dan geliyorsa: kullanıcı `config.toml`'u elle
  düzenliyorsa Nova onu **yeniden yazmaz** (yorumlar korunur). Bu durumda
  `state.toml`'a `hidden_tools = ["id"]` yazılır ve araç gizlenir; ekranda
  "config.toml'da duruyor, Nova'da gizlendi" denir. `state.toml`'da
  tanımlanmış (form ile eklenmiş) araçlar için `state.toml`'daki
  `[[custom_tools]]` kaydı gerçekten silinir.
- **Güven sınırı**: Nova hiçbir komutu kabuk üzerinden çalıştırmaz; zaten
  tanımlı bir programı doğrulanmış dizinde yeni bir terminal sekmesinde
  başlatır. Özel araç = kullanıcının kendi sisteminde, kendi hesabıyla
  çalışan bir program. Nova sisteme kurulum yapmaz, yetki yükseltmez, sudo
  kullanmaz, argümanları yorumlamaz. Form ekranının altında bu sınır tek
  satırda yazar.

### 2.5 Proje dizinleri

- `state.toml`'da `project_dirs = ["/path/a", "/path/b"]` (ekleme `P` tuşu
  ile form benzeri tek satır; silme onaylı).
- Başlatma anında: katalog girdisinin `working_directory` değeri
  `Project(<yol>)` ise **geçerli proje dizini** (App'te seçili olan) kullanılır;
  böylece opencode/codex/lazygit tıpkı elle tanımlandığı gibi seçili dizinde
  açılır. Seçili proje dizini yoksa `Project` girdileri normal çalışma dizinini
  kullanır ve durum satırında bunu söyler.
- Proje dizini seçici: `P` tuşu listeyi açar, Up/Down seçer, Enter seçili
  dizini **geçerli** yapar; `A` yeni dizin ekler, `D` siler (onaylı).
  Başlıkta geçerli proje dizini kısa yolu görünür.
- Var olmayan dizin: kaydetmek serbesttir (kullanıcı sonra oluşturabilir).
  Başlatma anında dizin yoksa: **hiçbir şey oluşturulmaz**, açık hata
  gösterilir ve "dizini oluşturmak için onay ister misiniz?" sorusu
  (onay ekranı) çıkar. Onaylanmışsa `fs::create_dir_all` çağrılır (yalnızca
  tek seviye değil, eksik üst dizinler de; sonuç durum satırında açıkça
  bildirilir, oluşturma başarısızsa hata). Onay verilmezse hiçbir dosya
  sistemi değişmez.

### 2.6 Klavye düzeni (tek tablo, hem UI hem README)

| Tuş | Bağlam | İşlev |
|---|---|---|
| `↑`/`↓`, `j`/`k` | her yer | Gezinme |
| `←`/`→`, `h`/`l` | ana ekran | Kategori ↔ araç panelleri |
| `/` | ana ekran | Arama |
| `Esc` | her modal | Kapat / vazgeç |
| `Enter` | araç | Başlat |
| `F` | araç | Favori aç/kapa |
| `C` | ana ekran | Favori filtresi |
| `R` | ana ekran | Geçmiş |
| `P` | ana ekran | Proje dizinleri |
| `N` | ana ekran | Yeni özel araç |
| `D` | özel araç | Sil (onay) |
| `?` | ana ekran | Yardım |
| `Q` / `Ctrl+C` | ana ekran | Çıkış |

Modal (form/seçici/onay) açıkken ana ekran kısayolları **çalışmaz**;
böylece yıkıcı işlem yanlışlıkla tetiklenmez.

### 2.7 Ayarlar

`[ui]` (kullanıcı `config.toml`'u) ve `[prefs]` (`state.toml`, Nova yazar):

- `[ui] recent_limit` — geçmiş sınırı.
- `[prefs] preferred_terminal = "konsole"` — desteklenen adapterler arasında
  tercih. Yalnızca Konsole var; bulunamazsa açık hata ("tercih edilen
  terminal konsole bulunamadı, sistemde arayacağım"). Başka terminal için
  "desteklenmiyor" hatası.
- `[prefs] default_working_dir = "/yol"` — `Current`/`Home` yerine
  kullanılacak dizin. Geçersizse açık hata, sessiz düşüş yok.
- Yardım ekranında etkin dosya yolları gösterilir:
  `config: $XDG_CONFIG_HOME/nova/config.toml`, `state: $XDG_CONFIG_HOME/nova/state.toml`
  (ortam değişkeniyle değiştirilebilir: `NOVA_CONFIG`, `NOVA_STATE`; yalnızca
  testlerde kullanılır).

## 3. Modüller ve tipler

```
src/config/mod.rs      : + StateFile, FavoriteState, RecentEntry, Prefs,
                         load_state, save_state, state_path, slugify (catalog'a?)
src/catalog/loader.rs  : + merge_with_state (favoriler, gizli, özel araçlar)
src/app/state.rs       : AppState (favorites set, recents, project_dirs, prefs)
                         + commit/save orkestrasyonu
src/app/modal.rs       : Modal { None, Confirm(..), ToolForm(ToolForm),
                         ToolReview(..), ProjectPicker(..), History(..) }
src/app/form.rs        : ToolForm alanları, doğrulama, özet
src/app/mod.rs         : handle_key -> modal dispatch
src/ui/modal.rs        : modal çizimi
src/launcher/terminal.rs: select_adapter(preferred: Option<&str>)
src/launcher/mod.rs    : launch_with(.., options: &LaunchOptions)
src/launcher/request.rs: LaunchOptions { project_dir, default_dir }
```

`AppState` `state.toml`'un **bellek içi modeli** olur; serde tipleri `config`
tarafında kalır (Faz 3'teki "serde ≠ mantık" kuralı korunur).

## 4. Uygulama planı

Her adım: kod → test → gate (fmt, build, clippy -D warnings, test). Adımlar
sıralı; her birinin sonunda yeşil olmalı.

- [x] A1 `state.toml` serde tipleri + atomik okuma/yazma + bozuk dosya
      yedekleme. Birim testleri.
- [x] A2 Favorilerin kalıcılığı: yükleme sırasında state uygula, değişimde
      kaydet; varsayılan tohum yalnızca ilk çalıştırma.
- [x] A3 Geçmiş: sınır, tekrar birleştirme, yalnızca başarılı başlatmada
      kayıt, `R` görünümü + onaylı temizleme.
- [x] A4 Proje dizinleri: `P` seçici, ekleme/silme, launcher'a aktarım,
      dizin yoksa onaylı oluşturma.
- [x] A5 Özel araç formu: alanlar, doğrulama, gözden geçirme, kaydetme;
      özel araç silme (onaylı) + gizli araç davranışı.
- [x] A6 Ayarlar: `preferred_terminal`, `default_working_dir`, `recent_limit`;
      `select_adapter` tercih parametresi.
- [x] A7 UI: modal çizimi, geçmiş/proje ekranları, hata/başarı renkleri,
      yardım ekranındaki tuş tablosu + dosya yolları, dar mod uyumu.
- [x] A8 Entegrasyon testleri (`tests/personalization_test.rs`), README +
      ARCHITECTURE güncellemesi, PTY duman testi, gate raporu.

## 5. Test stratejisi

- Birim: `state` okuma/yazma (bozuk, eksik, atomiklik), `slugify`,
  `ToolForm::validate`, `RecentList` sınırı ve zamanlama.
- Entegrasyon (`tui_experience_test` genişletme): favori F → uygulama yeniden
  yükle → favori hâlâ orada; başarısız başlatma geçmişe yazmaz; özel araç
  form → gözden geçir → kaydet → listede; eksik alan → hata, kayıt yok;
  silme onayı Esc → silinmemiş; proje dizini seçimi → `LaunchRequest`
  içindeki dizin doğru.
- Launcher: `select_adapter(Some("konsole"))`, bilinmeyen tercih hatası.
- Bozuk `state.toml` → yedek dosyası oluşur, eski dosya okunamaz hâle
  gelmesin.

## 6. Kapsam dışı (ilk sürüm)

- Gömülü araçların kullanıcı tarafından silinmesi (yalnızca gizleme yok
  ilk sürümde; gömülü araçlar için D tuşu kapalı).
- Fare desteği (klavye önceliği, fare klavyeyi bozmamalı).
- Konsole dışındaki terminaller.
- Yerel saat biçimlendirme (göreli zaman yeterli).
- Ayar ekranı (dosyalar elle düzenlenir; yardım ekranı yolu gösterir).
