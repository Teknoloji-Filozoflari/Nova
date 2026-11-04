# Nova Mimari Belgesi (Faz 5)

Bu belge Nova projesinin kritik mimari tercihlerini, modül sınırlarını ve tasarım kararlarını açıklar. Faz 0 (iskelet + terminal dayanıklılığı), Faz 1 (TUI deneyimi + katalog), Faz 2 (yeni Konsole sekmesinde başlatma), Faz 3 (TOML kataloğu, kullanıcı yapılandırması, başlatma türleri), Faz 4 (kişiselleştirme, kalıcı durum, özel araçlar) ve Faz 5 (test, uyumluluk, yayın hazırlığı) kararlarını birlikte kapsar.

## Genel Yaklaşım

Nova, mevcut terminal araçlarını yeniden yazmak yerine bunları keşfetmek, kataloglamak ve başlatmak için bir üst katmandır. Kullanıcı arayüzü (TUI), araç tanımları (katalog) ve başlatma mantığı (launcher) kesinlikle ayrı tutulur.

Katalog modeli UI'dan bağımsızdır: `catalog` ve `catalog::loader` modülleri `ratatui` veya `crossterm` tiplerini tanımaz, bu yüzden aynı model TOML'dan, testlerden veya başka bir arayüzden kullanılabilir.

## Modül Sınırları

| Modül | Sorumluluk | Bağımlılıklar |
|-------|-----------|--------------|
| `app` | Uygulama durumu, ana döngü, olay yönlendirme, tuş işleme, filtreleme kararları, modal akışları | `catalog`, `ui`, `event`, `error`, `config::state` |
| `app::state` | `AppState`: favoriler/geçmiş/proje dizinleri yardımcıları, yol normalizasyonu, göreli zaman | `config::state` |
| `app::form` | Özel araç formu: alanlar, doğrulama, gözden geçirme satırları, kimlik üretimi — yan etkisiz | `catalog`, `config` |
| `app::modal` | Modal durumları, ayar/form ekranları ve onay eylemleri (saf veri) | `app::form` |
| `ui` | Ratatui ile ekran çizimi, uyarlanabilir düzen, metin kesme | `app` (salt okunur referans) |
| `event` | Klavye / terminal olaylarını okuma ve sınıflandırma | `crossterm` |
| `error` | Hata türleri (`NovaError`) ve dönüşümleri | `std`, `toml` |
| `config` | TOML yapıları (serde tipleri), başlatma türü, kullanıcı dosyası yolu, okuma/yazma | `serde`, `toml` |
| `config::state` | `state.toml` şeması, atomik yazma, bozuk dosya yedekleme | `serde`, `toml` |
| `catalog` | Araç modeli, filtreleme, arama, Türkçe normalizasyon, çalışma dizini modeli, `ToolEntry → CatalogEntry` dönüşümü ve doğrulaması | `config`, `detection`, `launcher::resolve_directory` |
| `catalog::loader` | Gömülü katalog (TOML, `include_str!`), ayrıştırma, kullanıcı dosyasıyla `id` üzerinden birleştirme | `catalog`, `config` |
| `launcher` | Başlatma orkestrasyonu: istek → adapter → ayrık süreç | `catalog`, `error`, `which` |
| `launcher::request` | `LaunchRequest` ve çalışma dizini doğrulaması (yan etkisiz; `from_entry_with_project` etkin projeyi geçersiz kılar) | `catalog`, `error` |
| `launcher::terminal` | `TerminalAdapter` trait'i ve emülatör adapterleri (`$TERMINAL` + bilinen liste; yalnızca argüman üretir) | `launcher::request`, `error` |
| `detection` | PATH **ve mağaza `bin` dizinleri** üzerinden kurulum durumunu belirleme (üç durumlu) | `which`, `store` |
| `store` | Mağaza kataloğu modeli, kategori listesi, katalog doğrulama kuralları, kurulum dizini çözümü | `serde` |
| `store::loader` | Gömülü mağaza kataloğu (`assets/store/*.toml`, `include_str!`) ve doğrulaması | `store` |
| `store::install` | SHA-256 doğrulama, yol geçişi korumalı arşiv çıkarma, kullanıcı alanı kontrolü, kurulum orkestrasyonu | `store`, `sha2`, `flate2`, `tar` |
| `store::http` | Gerçek HTTPS indirici (`ureq`); gövdeyi dosyaya akıtır, boyut sınırlar | `store::install`, `ureq` |
| `store::manifest` | Canlı sürüm takibi: `ReleaseSource`, `ResolvedRelease`, `ReleaseProvider` arayüzü, önbellek (`ETag` + TTL) | `update::Version`, `serde` |
| `store::github` | GitHub Releases çözümleyicisi: API sorgusu, `ETag` ile koşullu istek, upstream checksum dosyası okuma | `store::manifest`, `serde_json` |
| `file_picker` | Salt okunur dosya seçici durumu (dizin gezinme, sıralama, seçim) | — |
| `media_launcher` | Dosyayı yapılandırılmış uygulamayla **kabuksuz** açma (ayrı argüman) | `error` |
| `update` | Sürüm karşılaştırma (`semver`-uyumlu, yerel), güncelleme sağlayıcı arayüzü | — |

**Kural:** `ui` modülü sadece çizer; iş mantığı içermez. `app` modülü durum tutar, karar verir ve `ui` üzerinden render edilir.

Bir istisna: `ui` içindeki `truncate()` ve genişlik eşikleri çizimle doğrudan ilgili olduğu için `ui`'de durur, ancak bunlar saf fonksiyonlardır ve birim testleri vardır.

## Araç Kataloğu ve Yapılandırma (Faz 3)

### İki kaynak, tek model

| Kaynak | Yol | Sahibi | Güncelleme |
|--------|-----|--------|------------|
| Gömülü varsayılan | `assets/catalog/*.toml`, `include_str!` ile ikiliye gömülür | Uygulama | Değişebilir |
| Kullanıcı | `config::user_config_path()` → `$XDG_CONFIG_HOME/nova/config.toml` (yoksa `~/.config/nova/config.toml`) | Kullanıcı | Dokunulmaz |

Bu ayrım "kullanıcı verisi uygulama varsayılanlarından ayrıdır" ilkesinin uygulanışıdır: güncelleme yalnızca gömülü dosyayı değiştirir, kullanıcının eklediği araçlar ve favoriler yerinde kalır.

### Sorumluluk paylaşımı: serde tipleri vs. model

- `config` yalnızca **serde tiplerini** tanımlar: `ToolEntry`, `CatalogFile`, `UserConfig`, `UiConfig`, `LaunchKind`. Bunların tüm alanları `#[serde(default)]`'tir, böylece kullanıcı yalnızca değiştirmek istediği alanı yazabilir (`executable`, `id = "command"` takma adıyla da kabul edilir).
- `catalog::loader` dosyaları okur, ayrıştırır, birleştirir ve **ortaya çıkanı doğrular**.
- `catalog::catalog_entry_from_tool` tek bir `ToolEntry`'yi `CatalogEntry`'ye çevirir ve iş kuralı doğrulamasını yapar (boş `id` yok, kategori bilinen sekizden biri, `executable` yol değil, boş argüman yok, `command_help` için boş olmayan `help_text`).

Bu bölünme bağımlılık döngüsünü önler: `catalog` → `config`, tersi değil. `loader`'ın `catalog`'a bağımlı olması sorun değildir, çünkü `catalog` onu tanımaz.

### Birleştirme kuralı (`id` anahtarı)

`loader::merge_with_user(builtin_entries, user_config)`:

1. Gömülü kayıtların her biri için aynı `id`'ye sahip kullanıcı kaydı varsa `CatalogEntry::merge_user` uygulanır.
2. Kullanıcı dosyasında olmayan gömülü kayıtlar olduğu gibi kalır.
3. Yalnızca kullanıcı dosyasında olan kayıtların hepsi `catalog_entry_from_tool` ile doğrulanıp listenin sonuna eklenir; `origin = User` ile işaretlenir.
4. Sonuç `validate_merged` ile tekrar kontrol edilir (`validate_merged`, serde katmanını atlayan yollar için de geçerlidir).

`merge_user` ayrıntıları — hepsi birim testiyle kilitli:

| Alan | Kural | Gerekçe |
|------|-------|---------|
| `name`, `description`, `category`, `executable`, `help_text`, `info_text`, `alternative_to` | Boş değilse kullanıcı kazanır, boşsa varsayılan kalır | Kısmi yapılandırma (`id` + `favorite`) mümkün olsun |
| `args` | Yalnızca **boş değilse** değiştirilir | `args = []` yazmak "varsayılan argümanları sil" anlamına gelmemeli |
| `favorite` | Her zaman kullanıcıdan | Kalıcı favori tohumu |
| `launch_kind` | Varsayılan `interactive` değilse, ya da zaten `interactive` ise uygulanır | Kullanıcı bilinçli olarak `interactive` verip varsayılanı ezebilsin |
| `working_directory` | `WorkingDirChoice::Plain` ise uygulanır | `custom`/`project` yol gerektirir; yolsuzsa `Invalid` ile reddedilir |

`validate_merged` kategori dışı değerleri, `/` içeren `executable`'ları ve `help_text`'siz `command_help` kayıtlarını reddeder. `Catalog::from_entries` ayrıca boş katalog ve yinelenen `id` durumunu reddeder.

### Hata mesajları

Ayrıştırma hatası dosya adını ve `toml`'un verdiği satır/sütunu taşır:

```
'<yol>' geçerli TOML değil: TOML parse error at line 1, column 8 ...
```

İş kuralı hatası hangi `id`/alanla ilgili olduğunu söyler. `main()` bu metni `NovaError::Config` içine sarar ve **kataloğu alt ekrana geçmeden önce** yükler, böylece hata normal terminalde okunabilir kalır.

### Gevşeklik kararı: kullanıcı dosyası araçsız olabilir

Kullanıcı dosyası yoksa yalnızca gömülü katalog kullanılır. Dosya varsa ama `tools` boşsa (`[ui]` ayarı dışında bir şey yok), birleştirme **atlanır** ve gömülü katalog döner. Aksi hâlde `merge_with_user` "katalog boş olamaz" hatası verirdi — oysa kullanıcı yalnızca bir ayar değiştirmiş olabilir. Bozuk TOML hâlâ hatadır.

### Test enjeksiyonu

`loader::load_with_builtin(entries, user_path)` gömülü katalog yerine verilen listeyi kullanır. `load(path)` bundan ibarettir. Entegrasyon testleri `builtin_catalog()` + `Catalog::from_entries` ya da `load_with_builtin` kullanır; hiçbir test kullanıcının gerçek `config.toml` dosyasına bağımlı değildir.

## Başlatma Türü (`LaunchKind`) — `LaunchMode`'dan ayrı

`LaunchMode` (`Terminal` / `Background` / `Detached`) *nerede* çalıştırılacağını, `LaunchKind` ise *ne* yapılacağını söyler:

| `LaunchKind` | Davranış | `LaunchAction` |
|--------------|----------|----------------|
| `Interactive` (varsayılan) | Program olduğu gibi yeni sekmede başlatılır | `Program` |
| `CommandHelp` | Araç **çalıştırılmaz**; TOML'daki sabit `help_text` bir sayfalayıcıya verilir | `CommandHelp` |
| `ShellIntegrationInfo` | Araç çalıştırılmaz; `info_text` görünümü açılır | `IntegrationInfo` |
| `CustomCommand` | Kullanıcının `args`'ı aynen uygulanır | `Program` |

`CommandHelp` için `--help` çalıştırılmaz: bilinmeyen bir araçta alt süreç başlatmak davranışı öngörülemez kılar; metin katalogla birlikte sürümlenir. `LaunchKind::mark()` yalnızca `Interactive` dışındaki türler için detay alanında işaret üretir.

`CustomCommand` dışındaki türlerde kullanıcı `args`'ı birleştirme sırasında uygulanır (yukarıdaki tablo); bu, "varsayılan argümanlar yalnızca gerekli ve güvenli olduğunda" ilkesinin koddaki karşılığıdır.

### stdin sorunu ve geçici dosya

Konsole, `-e` ile başlattığı programa **stdin yönlendiremez** (gerçek Konsole 26.08.2 üzerinde doğrulandı). Sayfalayıcıya metin vermek için kabuk kullanmak (`sh -c "… | less"`) yasaktır. Çözüm:

1. `LaunchRequest::set_stdin(bytes)` isteğe metin koyar.
2. `write_stdin_file()` `std::env::temp_dir()` altında benzersiz, `0700` izinli bir klasör oluşturur; içindeki `help.txt` dosyası `create_new` ve Unix'te `0600` izinleriyle açılır.
3. Konsole, Nova'yı dahili sayfalayıcı modunda açar. Bu mod sayfalayıcıyı dosyayla başlatır ve sayfalayıcı kapandıktan sonra dosyayı siler.
4. Sayfalayıcı `handlers: ["less", "more"]` listesinden PATH'te bulunan ilk adaydır. `find_handler` hiçbirini bulamazsa başlatma açık hatayla reddedilir; `cat` kullanılmaz (tek sayfada kaybolur, kaydırma yok).

Dosya, Konsole'nin asenkron olarak okumasına fırsat vermek için sayfalayıcı kapanana kadar tutulur. Sayfalayıcı kapanınca dosya ve boş klasör silinir. Konsole isteği başarısız olursa Nova oluşturduğu dosyayı temizler; Konsole dahili modu hiç başlatamazsa dosya ve klasör geçici dizinde kalabilir.

## Kurulum Durumu Modeli (üç durumlu)

`detection::InstalledStatus` üç değerlidir:

| Değişken | Anlamı |
|----------|--------|
| `Installed` | Çalıştırılabilir dosya PATH üzerinde bulundu, `resolved_path` dolu |
| `Missing` | Arandı ve bulunamadı |
| `Unknown` | Durum belirlenemedi (boş komut adı, PATH okunamadı vb.) |

Kritik detay: `CatalogEntry::new(...)` durumu **her zaman** `detection::resolve` ile çözer. Hiçbir girdi sessizce `Unknown` varsaymaz; `Unknown` yalnızca gerçekten belirlenemediğinde oluşur. Bunun sonucu olarak `Catalog::missing()` yalnızca `Missing` girdileri döner — `Unknown` olanlar güvenilmez olduğu için "kurulu değil" sayılmaz.

Doğrulama yalnızca PATH taramasıdır (`which`). Hiçbir araç tespit için çalıştırılmaz, `--version` bile çağrılmaz. `executable` boşsa komut adı `id`'den gelir; bu da boşsa `Unknown` olur. Tespit sonucu araç eksik olsa da kayıt listede kalır (`✗` ile görünür).

`symbol()` / `label()` UI metnini üretir, fakat renk seçimi `ui::status_color()` içindedir; model renk bilmez.

## Arama ve Normalizasyon

Arama dört alanda yapılır: ad, komut adı, kategori, açıklama. Karşılaştırma `catalog::normalize()` üzerinden yapılır.

`normalize()` yalnızca `to_lowercase()` değildir; Türkçe için iki düzeltme yapar:

- `İ` → `i` + birleşik nokta (`U+0307`) olarak açılır; birleşik nokta atılır. Aksi hâlde `"SİSTEM"` araması `"Sistem"` ile eşleşmez.
- Büyük `I` ASCII `i` (`U+0069`) olur, ancak Türkçe küçük `ı` `U+0131`'dir; `ı` da `i`'ye çevrilir. Aksi hâlde `"BAĞLANTI"` araması `"Bağlantı"` ile eşleşmez.

Bu, tam bir Unicode case-folding değildir; arama amacıyla yeterli, öngörülebilir bir sadeleştirmedir. Hem `Catalog::search()` hem `App::matches_search()` aynı fonksiyonu kullanır, böylece iki arama yolu birbirinden ayrışmaz.

**Arama ile kategori ilişkisi:** Arama sırasında seçili kategori filtresi uygulanmaz. Aksi hâlde kullanıcı "AI ve Kodlama" seçiliyken `tmux` gibi başka kategorideki bir aracı asla bulamazdı. Arama favori filtresinin üzerine uygulanır (favoriler açıksa yalnızca favoriler içinde arar).

## Uyarlanabilir Düzen (`ui`)

Düzen terminal boyutuna göre üç genişlik moduna ve bir yükseklik eşiğine sahiptir. Eşikler `const` ve derleme zamanında tutarlılığı `const` bloklarıyla doğrulanır:

| Bölge | Eşik | Davranış |
|-------|------|----------|
| Geniş | ≥ `WIDE_MIN_WIDTH` (70) | **Kurulu / Ayarlar:** sol sütun `prefs.pane_secondary_width` (varsayılan 28, `[` `]`). **Mağaza / Komut Rehberi:** sekme (26) + ikincil sütun (aynı pref) + liste (`prefs.pane_list_width`, varsayılan 32, `-` `=`); yan detayda fazla genişlik detaya gider. Sütun tuşları her sekmede çalışır |
| Orta | 45–69 | Tek sütun; kategori adı araç paneli başlığında; detay görünür. Mağaza sekmesinde kategoriler üstte, uygulamalar altta |
| Dar | < 45 (`MEDIUM_MIN_WIDTH`) | Yalnızca araç listesi; detay gizli; durum satırında yol gösterilmez |
| Sıkı ipucu | < 52 (`TIGHT_HINT_WIDTH`) | Arama/yardım/durum metinleri kısa biçime düşer |
| Kısa yükseklik | < 14 (`TALL_MIN_HEIGHT`) | Yardım paneli tek satıra iner |
| Yardım açık | `CONTENT_MIN_HEIGHT` (5) + `HELP_HELP_MAX_HEIGHT` (10) | Panel tuş tablosu, dosya yolları ve çalışma dizini için büyür |

Detay panelinin yüksekliği sekmeye göre değişir: araç detayı 8 satır
(`DETAIL_ROWS`), mağaza detayı 13 satır (`STORE_DETAIL_ROWS`) — mağaza detayı
sürüm, canlı takip, kurulum yöntemi ve güven sınıfı (A/B/C/D) taşır. Mağaza
detayı ekrana sığmayacak kadar büyürse içerik alanı korunacak şekilde
kırpılır; yardım paneli büyürken mağaza sekmesinde içerik alanı daralmaz.

Yardım panelinin yüksekliği iki durumda hesaplanır. Normalde `tall` ise 3 satır, değilse 1 satırdır. `?` ile yardım açıldığında panel `height - (başlık + arama + CONTENT_MIN_HEIGHT + detay + durum)` satır alır, en fazla `HELP_HELP_MAX_HEIGHT` olacak şekilde sınırlanır ve en az 1 satırdır. Böylece araç listesi 5 satırdan aza düşmez, tuş tablosu da kırpılmaz. Yükseklik yettiğinde `help_full_lines` 8 satır basar: 5 tuş satırı + `config`/`state` yolları + çalışma dizini. Dar genişlikte (`< WIDE_MIN_WIDTH`) `help_full_lines_narrow` tek sütunlu, en sık kullanılan tuşları önce veren bir liste kullanır; kırpılma kaçınılmazsa en az kritik tuşlar kesilir. Sıkı genişlikte tek satıra indirme (`short_line`) yalnızca yardım kapalıyken uygulanır — açık bir yardım ekranı tek satıra çökmez. Bu davranışın kırpma geri dönüşü `tests/tui_experience_test.rs` içindeki `help_screen_shows_full_key_table_and_paths_when_open` ve `help_screen_renders_without_panic_on_small_terminals` testleriyle korunur.

Tüm metin kesme işlemleri `truncate()` ile karakter bazında yapılır (bayt değil), bu yüzden Türkçe karakterlerde ve emoji genişliğinde panik oluşmaz. `Paragraph` widget'ı alan kenarında zaten kırpar; bu nedenle satırlar ayrıca kesilmek zorunda değildir.

`narrow_mode` `app` durumunda tutulur, çünkü tuş davranışını da etkiler (dar modda `h`/`←` odak değiştirmez). `Resize` olayında güncellenir ve dar moda geçilirse odak araç paneline alınır.

## Kullanıcı Geri Bildirimi

`app.status_message` anlık geri bildirim taşır: favori eklendi/çıkarıldı, arama sonucu sayısı, başlatma reddi, "başlatma henüz bağlı değil" mesajı.

İki kural:

1. Boşluk, `status_message` yoksa gösterilir. Mesaj her zaman seçili araç bilgisinin **üzerinde** önceliklidir — aksi hâlde kullanıcı eyleminin sonucunu göremez.
2. `↑`/`↓` gezinme mesajı temizler, böylece eski bir mesaj kurulum durumunu kalıcı olarak gizlemez.

`Enter` duruma göre üç farklı mesaj üretir (kurulu → başlatma isteği, kurulu değil → red, bilinmiyor → red). Faz 2'den itibaren kurulu araç için mesaj `launcher`'dan gelen sonuçtur; kurulu olmayan / bilinmeyen durumda `app` başlatıcıyı **hiç çağırmaz**, çünkü kullanıcıya en yakın yerde reddetmek gereksiz bir emülatör aramasını da önler. Bu kontrol `launcher` içinde ayrıca tekrarlanır (savunma katmanı).

## Başlatma Mimarisi (Faz 2)

Katmanlar, her biri tek bir soruya cevap verecek şekilde ayrılmıştır:

| Katman | Soru | Yan etki |
|--------|------|----------|
| `LaunchRequest::from_entry` | *Ne*, *hangi argümanlarla*, *hangi dizinde*? | Yok (süreç başlatmaz) |
| `TerminalAdapter::build_args` | Emülatör bu isteği nasıl komut satırına çevirir? | Yok (yalnızca `Vec<OsString>`) |
| `ProcessSpawner::spawn` | Emülatör süreci başladı mı? | Süreç oluşturur, PID döner |
| `LaunchOutcome` | Kullanıcıya ne söylenebilir? | Yok |

Bu ayrımın amacı, testlerin gerçek bir Konsole açmadan tüm yolu sınaması ve "istek oluşturuldu" ile "uygulama açıldı" iddialarının karışmamasıdır.

### Terminal adapter komutu (plugin sistemi yoktur)

Yeni bir emülatör eklemek `TerminalAdapter` trait'ini uygulayan bir struct yazmak ve `TERMINALS` listesine bir satır eklemekten ibarettir. Dinamik yükleme, harici yapılandırma veya plugin keşfi yoktur — bu, Faz 2 kapsamı dışında bırakılan bilinçli bir sadeleştirmedir.

`preferred` boşken önce `$TERMINAL` (basename bilinen bir adapterse) denenir; sonra `TERMINALS` sırayla `which` ile aranır. Hiçbiri bulunamazsa hata, **aranan** komut adlarını içerir.

### Konsole adapteri ve doğrulanmış argüman sırası

```
konsole --new-tab --workdir <dizin> -e <program> <argümanlar...>
```

- `-e` Konsole'de **tüm izleyen argümanları yakalar**, bu yüzden son seçenek olmak zorundadır. Birim testi (`konsole_dash_e_is_last_option_then_program`) bu sırayı kilitler.
- `--new-tab`: mevcut Konsole penceresinde sekme ister. Hiç pencere yoksa Konsole yeni bir pencere açar; yine de Nova'dan bağımsızdır.
- `--workdir` **geçersiz bir dizin verildiğinde hata vermez, sessizce `$HOME`'a döner** (gerçek Konsole 26.08.2 üzerinde doğrulandı). Bu yüzden dizin doğrulaması Nova'da zorunludur.
- Aracın programı ve argümanları kabuktan geçmez; `Command::new(program).args(...)` ile ayrı ayrı verilir.

### Çalışma dizini modeli

`WorkingDirectory` (katalog modeli, UI'dan bağımsız):

| Değişken | TOML değeri | Davranış |
|----------|-------------|----------|
| `Current` / `Home` | `current` (veya boş) / `home` | `$HOME` |
| `Custom(PathBuf)` | `custom` + `working_dir_path` | Aracın kendi dizini (ör. bir Git deposu) |
| `Project(PathBuf)` | `project` + `working_dir_path` | Proje dizini |

`resolve()` → `resolve_directory(selection, home, validator, tool_name)`:

1. Aday yol `DirectoryValidator::validate` ile kontrol edilir (`is_dir` + `canonicalize`, sembolik bağlantı ve göreli parçalar çözülür).
2. Geçerliyse kanonik tam yol döner.
3. Değilse **asla** sessizce ev dizinine düşülmez; açık hata verilir. Kullanıcı yanlış dizinde açılmış bir araç görmemeli.

`DirectoryValidator` trait'i, `resolve_directory`'nin dosya sistemi olmadan test edilmesini sağlar; üretimde `FsValidator` kullanılır.

`WorkingDirectory::Current` için ev dizini boşsa (yani `$HOME` yoksa) da hata verilir; ev dizini "güvenli varsayılan" olarak varsayılmaz.

### Ayrık başlatma

`CommandSpawner`, emülatörü `stdin/stdout/stderr = Stdio::null()` ile başlatır ve **beklemez**. Gerekçe: Konsole bir kabuktan başlatıldığında çocuk süreç sonlanana kadar ön planda kalıp ebeveynin stdio'sunu tutarı; TUI bundan donmamalıdır. `child.id()` okunduktan sonra `drop(child)` yapılır.

Bu, Nova'nın süreci yeniden sahiplenmediği (reap etmediği) anlamına gelir; PID yalnızca raporlama içindir, süreç yönetiminin sahibi değildir.

### "İstendi" ile "açıldı" arasındaki sınır

`LaunchOutcome::message()` bilinçli olarak "açıldı" demez:

> `'<araç>' Konsole içinde yeni sekmede başlatılması istendi (dizin: <yol>) — sekmenin açıldığını Nova doğrulayamaz, Konsole penceresine bak.`

Süreç `spawn` edilmesi yalnızca Konsole ikilinin çalıştığını kanıtlar; yeni sekmenin açıldığını, aracın PATH'te bulunduğunu veya aracın kendisinin çalıştığını kanıtlamaz. Nova IPC ile sekme takibi yapmaz.

### Hata yönetimi

Başlatma hataları `NovaError::Launcher(String)` olarak döner ve `app::select_tool` bunu **yalnızca** `status_message`'a yazar. `App::handle_key` bu mesajlar için hata döndürmez; bu yüzden hatalı bir başlatma TUI'yi bozmaz veya sonlandırmaz. Ana döngüye yalnızca gerçek TUI/IO hataları (`NovaError`) çıkar.

Sıra önemlidir: kurulum durumu ve çalışma dizini, süreç başlatılmadan **önce** doğrulanır; emülatör araması da doğrulamalardan sonra yapılır. Böylece geçersiz bir girdi hiçbir zaman yarım kalmış bir Konsole sürecine yol açmaz.

### Test enjeksiyonu

- `launch_with(entry, home, &spawner)`: `ProcessSpawner` enjekte edilebilir (kaydeden sahte spawner). Gerçek Konsole gerekmez.
- `App::set_launcher(LaunchFn)`: `app` içindeki başlatıcı fonksiyon işaretçisi testlerde değiştirilir; `select_tool` yolunun tamamı (reddetme dahil) süreç başlatmadan sınanır.

## Ana Döngü (`app::run`)

1. `terminal.draw(|f| ui::draw(f, &app))` — ekranı yeniden çizer.
2. `EventReader::next_event()` — 100 ms zaman aşımıyla bir terminal olayı okur.
3. Olaya göre:
   - `Quit` → `running = false`, döngü biter.
   - `Key` → `app.handle_key()` — arama, gezinme, seçim, çıkış.
   - `Resize` → `app.narrow_mode` güncellenir.
   - `Ignored` → yoksayılır.

`App::handle_key` `pub`'dır: ana döngü ve entegrasyon testleri aynı yolu kullanır, böylece testler gerçek kullanıcı davranışını sınar.

Her karede yeniden çizim yapıldığı için yeniden boyutlandırma ayrı bir "durum" tutmaz; bir sonraki kare yeni boyutla çizilir. Düzenler `Constraint` tabanlı olduğundan taşma üretmez.

## Terminal Yaşam Döngüsü (`main.rs` + `terminal_guard.rs`)

Tüm geri yükleme mantığı `src/terminal_guard.rs` içindedir; `main.rs` yalnızca sırayı kurar (`TerminalGuard` RAII + panik hook'u):

0. `run()`, terminale dokunmadan önce kataloğu (`load_default`) ve `state.toml`'u okur; hata uyarıları alt ekrana girmeden görünür olmalıdır.
1. Panik hook'u terminal hazırlanmadan **önce** kurulur; önceki hook korunur ve panik mesajı yine yazdırılır. Hook, terminal hazırlık adımlarının paylaşılan ilerleme bilgisini izler.
2. Raw mode ve alt ekran geçişleri denenmeden önce guard'a işaretlenir. Hazırlık başarısız olursa yalnızca denenmiş adımlar geri alınır; raw mode açılamadıysa alt ekran çıkış dizisi yazılmaz.
3. Uygulama çalışır.
4. `Drop` — hata olsa bile — yalnızca işaretlenmiş `disable_raw_mode()` / `LeaveAlternateScreen` adımlarını çağırır. Geri yükleme işaretleri atomik olarak tüketildiğinden panic hook ile `Drop` aynı işi iki kez yapmaz.

Doğrulama: normal çıkışta `\x1b[?1049h`/`\x1b[?1049l` çifti görülür; gerçek terminali olmayan ortamda (PTY ustası kapanınca) Nova asılı kalmaz (bkz. Bilinen Eksiklikler).

## Hata Yaklaşımı

Tüm hatalar `NovaError` enum'ı üzerinden temsil edilir. `std::io::Error`, `toml::de::Error`, `toml::ser::Error` için `From` dönüşümleri mevcuttur. `main.rs` hata olduğunda `NovaError` döndürür ve `TerminalGuard`'ın `Drop` sayesinde terminal temizlenir.

## Test Stratejisi

| Katman | Dosya | Kapsam |
|--------|-------|--------|
| Birim (lib) | `src/**/mod.rs` `mod tests` | Normalizasyon, durum çözümü, `truncate`, düzen eşikleri, TOML ayrıştırma/birleştirme |
| Entegrasyon | `tests/catalog_test.rs` | Gömülü katalog içeriği (zoxide/HTTPie yok, başlatma türleri, alternatifler), bozuk TOML, kullanıcı dosyası birleştirme, kısmi kayıt, doğrulama hataları, Türkçe arama, tespit |
| Entegrasyon | `tests/tui_experience_test.rs` | `handle_key` üzerinden gezinme, arama, favori, Esc sırası, `Enter` davranışı (`set_launcher` ile enjekte edilen başlatıcı), boş kategori |
| Entegrasyon | `tests/launcher_test.rs` | İstek → adapter → spawn yolu: içinde boşluk/özel karakter olan yollar, geçersiz dizinler, eksik araç, emülatör yokluğu, kabuk kullanılmadığı, ayrık başlatma, `from_entry_with_project` proje geçersiz kılma |
| Entegrasyon | `tests/personalization_test.rs` | Favoriler, geçmiş, gizlenen araçlar, özel araç formu, silme, proje dizinleri, `state.toml` yazma/hata yolları |
| Entegrasyon | `tests/render_layout_test.rs` | `tool_row_widths` satır düzeni; dar/geniş eşiklerde sütun bölüşümü |
| Birim (`launcher`) | `src/launcher/*` `mod tests` | Konsole argüman sırası (`-e` son, stdin yolu en sonda), `resolve_directory` (sahte doğrulayıcı), hata metinleri, proje dizini geçersiz kılma |
| Birim (`terminal_guard`) | `src/terminal_guard.rs` `mod tests` | Taklit `TerminalState` ile normal/hata/panik yollarında geri yükleme sırası |

Gerçek Konsole süreci **hiçbir testte** başlatılmaz; gerçek ortam doğrulaması ayrıca ve elle yapılır. Entegrasyon testleri kullanıcının gerçek `config.toml` dosyasını okumaz.

## Geliştirme Akışı

- `cargo build` — derleme
- `cargo test` — tüm testleri çalıştır
- `cargo clippy --all-targets -- -D warnings` — lint kontrolü (uyarı = hata)
- `cargo fmt` / `cargo fmt --check` — kod biçimi

Bu dört kontrol her fazın kabul kriteridir.

## Kişiselleştirme ve Kalıcı Durum (Faz 4)

### İki dosya, iki sahip

| Dosya | Yazan | Neden ayrı |
|-------|-------|-----------|
| `config.toml` | Kullanıcı (elle) | Araç tanımları ve `[ui]`; Nova bunu **asla** yeniden yazmaz |
| `state.toml` | Nova | Favoriler, geçmiş, gizlenenler, form araçları, proje dizinleri, `[prefs]` |

Ayrımın amacı bir bütünlük kuralıdır: arayüzden yapılan her değişiklik (favori,
silme, geçmiş) yalnızca Nova'nın sahip olduğu dosyaya yazılır. Kullanıcının
elle yazdığı yorumlar, sıralama ve biçim bozulmadan kalır. "Silme" bu yüzden iki
farklı anlama sahiptir:

- **Form aracı** (`custom_tools`) → `state.toml`'dan gerçekten kaldırılır.
- **`config.toml` aracı** → dosyaya dokunulmaz; kimlik `hidden_tools` içine
  eklenir ve yalnızca Nova görünümünde gizlenir. Dosyadan silmek kullanıcının
  işidir.
- **Gömülü araç** → silinemez; açık hata mesajı döner.

### Yazma: atomik ve geri alınabilir

```
benzersiz kardeş dosya (create_new, 0600)  →  write_all + sync_all  →  rename(state.toml)
```

`sync_all` + `rename` sayesinde süreç yazma sırasında ölürse eski dosya
bütündür. Bellekteki değişiklik, kalıcı yazma başarılı olduktan sonra
"tamamlanmış" sayılır: `save_new_tool` ve `remove_tool` yazma hatasında
katalogdaki/durumdaki değişikliği geri alır; böylece ekranda görünen ile
diskte duran birbirinden ayrı düşmez.

### Bozuk dosya: kaybetme, bildir

`load_state` çözümleme hatasında dosyayı silmez; `state.toml.bozuk-<unix>`
adıyla yedekler, boş durumla devam eder ve `state_warning` döndürür. Aynı
ilke tek özel araç düzeyinde de uygulanır: `App::with_state` özel araçları
kataloğa geri yüklerken tek bir geçersiz kaydı atlar ve
`AppState::restore_warnings` üzerinden adını ve nedenini başlangıç mesajında
gösterir. Bir aracın bozukluğu diğerlerinin veya dosyanın kaybına yol açmaz.

### Katalog birleştirme sırası

1. Gömülü katalog (`include_str!`).
2. `config.toml` araçları, `id` üzerinden birleştirilir (`merge_user`).
3. `state.toml` özel araçları (`custom_tools`) — yalnızca kimliği katalogda
   yoksa `insert_entry` ile eklenir.

`insert_entry` kaydın `origin` alanını `EntryOrigin::User` yapar. Bu alan
silmeye izin verilip verilmeyeceğini belirler; `catalog_entry_from_tool`
varsayılanı `Builtin` olduğu için yalnızca kullanıcı eklemeleri burada
işaretlenir.

### Favoriler: kimlik anahtarı ve tohum kuralı

Favoriler `id` kümesidir, katalogdaki `CatalogEntry` kopyası değildir. Bu
sayede:

- Katalog güncellenince (araç eklenmesi/çıkarılması) favoriler korunur.
- Katalogdan kalkan bir aracın kimliği listede kalır; araç geri gelirse yine
  favoridir (yetim kimlik budanmaz).
- Gömülü katalogdaki `favorite = true` **yalnızca `state.toml` hiç yokken**
  tohumlanır (`first_run_seed`). Kullanıcı tüm favorileri kaldırdıysa dosya
  boş kalır ve bir sonraki açılışta yeniden doldurulmaz.

### Geçmiş (recents)

- Kayıt anı: kullanıcı araçta Enter ile başlatmayı denediğinde. Kurulu değil,
  kontrol edilemedi veya launcher isteği reddetti durumları da `rejected` olarak
  saklanır; terminal isteği gönderilirse `request_sent` yazılır.
- `request_sent`, harici uygulamanın sağlıklı çalıştığı anlamına gelmez; yalnızca
  terminal isteğinin gönderildiğini belirtir. Önceki şemasında durum alanı
  olmayan geçmiş kayıtları geriye uyumlu biçimde `request_sent` sayılır.
- `record_attempt` aynı `id`'yi önce kaldırır, başına ekler → en yeni önce,
  tekrarsız. Her araç için son denemenin sonucu tutulur.
- Sınır: `config.toml [ui] recent_limit` (yoksa `[prefs] recent_limit`, yoksa
  20; 1..=100 aralığına kırpılır). Aşan kayıtlar sondan düşürülür.
- `clear_recents` onay ister; onaylanınca dosyaya yazılır.

`at` alanı UNIX saniyedir. Görüntüleme `relative_time(now, at)` ile yapılır;
tarih kütüphanesi bağımlılığı eklenmedi, çünkü tek ihtiyaç "x dk önce"
etiketidir.

### Ayarlar

- `S` modalı, `prefs.preferred_terminal` ve `prefs.default_working_dir` değerlerini
  düzenler; `config.toml` ve `state.toml` yollarını gösterir.
- Terminal adı boş (otomatik: `$TERMINAL` sonra bilinen liste) veya desteklenen
  bir emülatör adı olmalıdır; desteklenmeyen adapter seçimi hata verir.
- Çalışma dizini normalize edilir ve var olan bir dizin olduğu doğrulanır.
  Ayar ekranı dizin oluşturmaz.
- Ayarlar tek bir atomik state yazımıyla saklanır. Yazma başarısız olursa önceki
  tercihler belleğe geri yüklenir.

### Proje dizinleri

`project_dirs` kalıcı bir listedir; `active_project` **oturumluktur** ve
`state.toml`'a yazılmaz. Bu bilinçli bir sınırdır: yanlışlıkla kalıcılaşan bir
proje seçimi, sonraki açılışlarda tüm başlatmaları sürpriz bir dizine taşırdı.
Yol normalizasyonu (`normalize_project_path`) `~` genişletir ve mutlak yol
üretir. Liste eklemek **dizin oluşturmaz**.

Eksik dizin akışı iki adımlıdır:

1. Launcher dizini yok olarak bildirir → `App` açık hata mesajı gösterir ve
   `ConfirmAction::CreateDir` modalını açar.
2. Kullanıcı `Enter` ile onaylarsa `create_dir_all` çağrılır; `Esc` ile hiçbir
   şey oluşturulmaz.

Onay olmadan dosya sistemi değişmez. Bu, "asla dosya sistemi değiştirme"
kuralının tek istisnasıdır ve kullanıcıya aittir.

### Form ve gözden geçirme

`app::form` saf bir veri yapısıdır: `ToolForm` alanları tutar, `validate`
doğrular, `to_entry` katalog kaydını üretir. Form **hiçbir yan etki yapmaz**
(dosya yazmaz, süreç başlatmaz); yalnızca `which`/`metadata` okur.

Doğrulama sonrası iki adımlı akış vardır: `ToolForm` → `ToolReview` → kayıt.
Gözden geçirme ekranı `review_lines` ile kaydedilecek **değerleri aynen**
gösterir, böylece "ne yazdım" ile "ne kaydedilecek" arasında fark kalmaz.

Argüman ayrıştırma bilinçli olarak `split_whitespace`'tir: kabuk yok, tırnak
yok, genişletme yok. `*` ve `$HOME` olduğu gibi argüman olur. Sınırı da aynı
yerden gelir: boşluk içeren bir yol tek argüman olarak yazılamaz.

Modal tuş işleme tek bir yazıcıdan geçer: `apply_modal_key` modalı
`mem::replace` ile dışarı alır, kararı yerel bir değişkende toplar ve sonucu tek
seferde geri koyar. Her modalın `Esc` kolu açıkça `take(Modal::None)` döndürür
(boş `Esc => {}` kolu eski bir hataydı: aynı modal geri konuyordu). Form tuşları
`self.modal`'a dokunmadan çalışır ve `FormOutcome` ile sonucu bildirir.

### Modal önceliği

`handle_key` önce `modal != None` kontrolü yapar. Modal açıkken ana ekran
kısayolları (`q`, `n`, `d`, `c`, `r`, `p`, `?`) çalışmaz; harfler form alanına
yazılır, onay ekranında tanınmayan tuşlar yok sayılır. Böylece modal içinde
`q` yazmak uygulamayı kapatmaz — kapanma yalnızca ana ekranda geçerlidir.

**Mağaza bu kuralın dışındadır:** mağaza modal değil, ana ekranın bir sekmesi
olduğu için ana ekran kısayolları (arama, `U`, gezinme) mağaza sekmesinde de
çalışır. Yalnızca katalogla ilgili tuşlar (`f` favori, `c` favori filtresi,
`d` sil/gizle) mağazada karşılığı olmadığı için bilinçli olarak yoksayılır ve
kullanıcıya durum satırında açıklanır.

## Bilinen Eksiklikler (Faz 5 sonu)

- **Terminal seçimi çoklu emülatör.** Otomatik sıra: `$TERMINAL` → xdg-terminal-exec → kitty → alacritty → wezterm → gnome-terminal → konsole → foot → xterm → tmux. Ayarlarda bu adlardan biri seçilebilir; bilinmeyen `$TERMINAL` basename sessizce yok sayılır.
- **Sekme açıldığı doğrulanmıyor.** Mesaj bilinçli olarak "istendi" der; IPC ile sekme izleme yok.
- **`background` / `detached` çalıştırma biçimleri yok**; reddedilir.
- **Form aracı düzenleme yok.** Var olan özel araç düzenlenemez; silinip yeniden eklenir.
- **Form türleri sınırlı.** Form `interactive` ve `custom_command` seçtirir; `command_help` ve `shell_integration_info` ek sabit metin gerektirdiği için elle `config.toml` içinde tanımlanır.
- **Form tek satırlık argüman kabul eder.** Boşluk/tırnak yorumlanmadığı için boşluk içeren yollar bu formdan tanımlanamaz; `config.toml` ile mümkündür.
- **Kullanıcı dosyası otomatik oluşturulmuyor.** Yoksa yalnızca gömülü katalog kullanılır; örnek dosya üretilmez.
- **Konsole dahili sayfalayıcı modunu başlatamazsa** yardım dosyası özel geçici klasöründe kalabilir. Dosya `0600`, klasör `0700` izinlidir; normal akışta sayfalayıcı kapanınca ikisi de silinir.
- **`Özel Araçlar` gömülü katalogda boş.** UI boş kategori mesajı gösterir; araçlar kullanıcı dosyasından veya formdan gelir. Gömülü `lazygit`, `Git ve Geliştirici Araçları` kategorisindedir.
- **SSH için ayrı profil ekranı yok.** Profiller `custom_command` kayıtları olarak TOML'da tanımlanır.
- **Cloudflare örneği varsayılan olarak 25 MB indirir** ve `curl` ortalama indirme hızını gösterir; `executable` ve `args` kullanıcı `config.toml` dosyasından değiştirilebilir.
- **Fare desteği ana ekran listelerinde.** Kategori, araç, mağaza ve Ayarlar/tema panellerinde tıklama (kenarlık + kaydırma penceresi hesaba katılır) ve kaydırma çalışır; dosya seçici / onay / form modallarında fare yok sayılır.
- **Arama sıralaması yok.** Sonuçlar katalog sırasındadır; alaka sırasına göre sıralama yapılmaz.
- **Süreç yeniden sahiplenilmiyor.** Konsole süreci Nova tarafından `wait` edilmez (bilinçli); PID yalnızca raporlama içindir.
- **Asgari Rust sürümü doğrulanmadı.** Bağımlılıkların `rust-version` alanları ≥1.74 öneriyor; yalnızca 1.99.1 ile derleme/test yapıldı.
- **Tema tercihleri uygulanır** (`varsayilan` / `acik` / `orman` / `amber`); Ayarlar sekmesinden döngülenir ve `state.toml`’a yazılır.
- **PTY kapanınca alt ekrandan çıkış dizisi yazılamaz.** Terminal (tty) yoksa `leave_alternate_screen` IO hatası verir; hook bunu bilinçli olarak yutar ve Nova panik üretmeden çıkar.
- **GitHub Releases ikili yayınları vardır**; crates.io / AUR / distro paketi henüz yok. Proje SPDX `MIT OR Apache-2.0` çift lisanslıdır (`LICENSE.md` seçim özeti; metinler `LICENSE-MIT` / `LICENSE-APACHE`). Akış: `docs/VERSIONING.md`, otomasyon: `.github/workflows/release.yml`.
- **Mağaza sürüm denetimi kullanıcı isteğine bağlı.** Otomatik denetim yok; kullanıcı mağazada `U` tuşuna bastığında GitHub Releases sorgulanır.
- **Mağaza kataloğu imzalı uzaktan güncellenebilir.** Varsayılan gömülü `assets/store/*.toml` (veya diskteki son doğrulanmış kopya) kullanılır. Mağazada `U` ile Nova-Store Releases’tan imzalı katalog denenir; **minisign imzası gömülü anahtarla doğrulanmazsa uzak veri uygulanmaz** (bkz. `store::remote`, "İmzalı Nova-Store kataloğu").
- **GitHub API hız sınırı kimliksiz IP başına saatte 60 istek.** `GITHUB_TOKEN` ile saatte 5000'e yükseltilebilir; 6 saatlik önbellek taze kayıtları ağa hiç çıkarmadığı için sınırı büyük ölçüde azaltır. `ETag` yalnızca bant genişliği tasarrufu sağlar (`304` de sınırdan düşer).
- **Mağazada 6 uygulama indirilebilir.** Kalan 13 kayıt `manual` yöntemdedir; upstream doğrulanabilir SHA-256 özeti yayımlamadığı için "Kur" eylemi sunulmaz.
- **Tek çalıştırılabilir dosya kurulur.** Arşivdeki man sayfaları, tamamlayıcılar ve ek belgeler kurulmaz; yalnızca katalogdaki `executable` yerleştirilir.
- **Güncelleme sağlayıcısı GitHub Releases’a bağlıdır** (`Teknoloji-Filozoflari/Nova`). Ayarlar→Güncelle / `U` denetimi durumu durum satırında ve modalda gösterir (`denetleniyor` / `yayın yok` / `güncel` / `yeni sürüm` / `hata`). Yayın yoksa (`404`) “ikili yayın yok” sonucudur — `cargo`/kaynak güncellemesi yapılmaz. Uygun Linux ikilisi varsa kullanıcı onayından sonra `~/.local/bin/nova` kurulur.
- **Bağımlılık güvenliği CI'da.** `cargo audit` ve `cargo deny check` (bkz. `deny.toml`) her PR/push'ta ve haftalık zamanlamada çalışır; Dependabot Cargo ve Actions güncellemelerini açar.

## Proje Dizinini Geçersiz Kılma (Faz 5)

`LaunchRequest::from_entry`, kaydın kendi `working_directory` değerini **hemen** çözer. Etkin bir proje dizini varken bu, kayıttaki eski/bayat `working_dir_path` yüzünden geçerli bir proje olsa bile başlatmanın reddedilmesine yol açıyordu. Çözüm: `from_entry_with_project(entry, home, project: Option<&Path>)` — `project` verilirse `Project(_)` seçimini doğrulamadan **önce** onunla değiştirir; `from_entry` bunu `None` ile çağırır. Aynı değişim `launch_text` yolunda da uygulanır. Etkin proje yokken bayat yol sessizce düşürülmez; açık hata verir.

## Mağaza Kurulum Mimarisi (Faz 6)

Mağaza kataloğu araç kataloğundan **ayrı** tutulur: `assets/catalog/*.toml`
"Nova ile gelen araçlar", `assets/store/*.toml` ise "Nova'nın kurabileceği
uygulamalar" listesidir. İkisi farklı yaşam döngülerine sahiptir — mağaza
kayıtları sürüm + özet + adres taşır, araç kataloğu taşımaz.

### Mağaza bir modal değil, bir sekmedir

Mağaza, ayrı bir katman (`Modal::Store`) olarak değil, **ana ekranın sekme
listesindeki son satır** olarak modellenmiştir. Böylece kullanıcı mağazayı
araç kategorileriyle aynı yerden bulur; ayrı bir kısayol keşfetmesi gerekmez.

| Katman | Soru | Görev |
|--------|------|-------|
| Sekme listesi (sol) | Hangi bölümdeyim? | `CATEGORIES` + "Mağaza" |
| Mağaza kategorileri (orta) | Hangi mağaza kategorisi? | `STORE_CATEGORIES` |
| Mağaza uygulamaları (sağ) | Hangi uygulama? | `Store::by_category` / `Store::search` |

Odak modeli bir **zincir**dir; `h`/`l` (ya da `←`/`→`) bu zincirde gezinir:

| Sekme | Zincir |
|-------|--------|
| Araç (varsayılan) | `Category` → `Tool` |
| Mağaza (geniş ekran) | `Category` → `StoreCategory` → `StoreApp` |
| Mağaza (dar ekran) | `StoreCategory` → `StoreApp` |

Dar ekranda üç sütun yan yana sığmadığından mağaza kategorileri **üstte**,
uygulamalar **alta** yerleşir; buna göre zincir de iki panele düşer.

`m` tuşu yalnızca bu sekmeye bir **kısayoldur** (`App::select_store_tab`); odağı
doğrudan uygulama listesine götürür ki kurulum eylemi hemen görünsün.

### U ile bağlama duyarlı denetim

`U` tuşu bağlama göre anlam değiştirir:

- Araç sekmesinde → Nova'nın **kendi** güncellemesini denetler.
- Mağaza sekmesinde → mağaza uygulamalarının **canlı sürümlerini** denetler.

Aynı tuşun iki farklı işi yapması kasıtlıdır: kullanıcı hangi bölümdeyse
o bölümle ilgili denetimi tetikler.

### Durum önbelleği

Mağaza satırlarının kurulum durumu her karede `which` çağrısıyla hesaplanmaz;
`App::store_status` önbelleğinden okunur. Önbellek uygulama başlangıcında ve
her kurulum sonrasında (`App::refresh_detection`) tazelenir.

### Katalog doğrulaması

`Store::from_entries` katalog yüklenirken şu kuralları uygular:

1. Kimlikler benzersiz, kategori `STORE_CATEGORIES` içinde olmalı.
2. `download_archive` kaydı **mutlaka** `download_url` ve `checksum`
   taşımalı; `checksum` 64 onaltılık karakter (SHA-256) olmalı.
3. `download_url` yalnızca `https://` şemasında olabilir.

Bu kurallar sayesinde "Kur" eylemi yalnızca gerçekten doğrulanabilir bir
indirme için sunulur. Doğrulanamayan uygulamalar `manual` yöntemdedir ve
resmî kurulum yönergesi gösterilir.

### Kurulum akışı (güvenlik sırası)

```
onay → indir → SHA-256 doğrula → güvenle çıkar → çalıştırılabiliri bul → yerleştir → temizle
```

- **Onay zorunludur.** Kurulum başlamadan önce uygulama, sürüm, kaynak, hedef
  dizin ve yapılacak işlemler gösterilir; `Enter` dışında bir yol yoktur.
- **İndirme kabuk değildir.** `ureq` ile doğrudan HTTP GET yapılır; gövde
  dosyaya akıtılır. `curl | sh` deseni yoktur, `sh -c` hiç çalıştırılmaz.
- **Doğrulama çıkarma öncesidir.** `sha2` ile hesaplanan özet, katalogdaki
  özetle birebir karşılaştırılır. Uyuşmazlıkta kurulum durur; **hiçbir dosya
  hedef dizine yazılmaz** ve geçici dosya silinir.
- **Çıkarma korumalıdır.** `tar` + `flate2` ile açılır; mutlak yollar ve `..`
  bileşenleri reddedilir, sembolik/hard link oluşturulmaz, girdi sayısı
  (10.000) ve toplam boyut (512 MB) sınırlıdır, dosya izinleri korunur.
- **Hedef kullanıcı alanındadır.** `is_user_area` hedefin `$HOME` ya da
  Nova'nın kendi veri dizini altında olduğunu doğrular. Dizin henüz yoksa
  yol sözlü olarak normalleştirilir, var olan en derin atası çözümlenir;
  `..` ile kaçış denemeleri reddedilir. `sudo`/root yok, sistem dizinlerine
  yazılmaz.

### Arka planda yürütme

İndirme ağ üzerinden yapıldığı için kurulum ana döngüyü kilitlememek adına
ayrı bir iş parçacığında yürür (`App::start_store_install`). Sonuç bir kanaldan
alınır ve ana döngü her karede `App::poll_install` ile kontrol eder. Bu sayede
Nova'nın TUI'si kurulum sırasında donmaz, durum satırında ilerleme görünür.

### Tespit entegrasyonu

Mağaza, uygulamayı `<store>/<id>/bin/` altına kurar; bu dizin PATH'te değildir.
`detection::resolve` PATH'te bulamazsa mağaza `bin` dizinlerini tarar. Böylece
mağazadan kurulan araç Nova'da "kurulu" görünür ve doğrudan başlatılabilir.

## Canlı Sürüm Takibi (manifest)

Mağaza kataloğu, uygulamaları "sabit sürüm" olarak değil, **canlı izlenen
kaynaklar** olarak tanımlar. Bu, "uzaktan gelen katalog" değildir; güvenlik
modeli bilinçli olarak tasarlanmıştır.

### Veri akışı

```
gömülü katalog (depo + dosya adı deseni)   ← değiştirilemez
        │
        ▼
GitHub Releases API (en güncel yayın)       ← yalnızca sürüm ipucu
        │
        ▼
upstream'in kendi checksum dosyası          ← bütünlük kaynağı
        │
        ▼
indirilen arşiv ↔ SHA-256 karşılaştırması  ← kurulumun ön şartı
```

### Neden güvenli

| Bilgi | Kaynağı | Saldırgan değiştirebilir mi? |
|-------|---------|-------------------------------|
| Hangi depo izlenecek | Gömülü `assets/store/*.toml` | Hayır |
| Arşiv/checksum dosya adı | Gömülü katalog | Hayır |
| En güncel sürüm | GitHub Releases API | Yalnızca ipucu verir |
| SHA-256 özeti | Upstream'in checksum dosyası | Hayır |

Ağdan gelen veri yalnızca "şu sürümü kontrol et" diyebilir. Upstream'in
yayımladığı özet değiştirilmeden tek bir bayt kurulamaz; çünkü doğrulama
**indirilen dosyaya** karşı yapılır.

### İmzalı Nova-Store kataloğu

Uygulama kataloğu artık yalnızca gömülü TOML’a kilitli değildir; ayrı depo
`Teknoloji-Filozoflari/Nova-Store` imzalı yayın üretebilir. Güvenlik modeli:

```
gömülü pubkey (Nova)  +  uzak store-catalog.toml + .minisig
        │
        ▼
minisign doğrula ──başarısız──► son güvenilen yerel / gömülü katalog (değişmez)
        │
     başarılı
        ▼
Store::from_entries + trusted-store.toml[.minisig] yaz
```

- Uzak adres stub: GitHub Releases `…/Nova-Store/releases/latest/download/store-catalog.toml`
  (+ `.minisig`). `NOVA_STORE_CATALOG_URL` / `_SIG_URL` ile override;
  `NOVA_STORE_CATALOG_DISABLE_REMOTE=1` uzak denemeyi kapatır.
- İmza doğrulanmadan katalog **asla** uygulanmaz; 404 / ağ / bozuk imza
  mevcut güvenilen kopyayı korur.
- Canlı sürüm takibi (uygulama başına GitHub Releases) ayrı kalır; katalog
  yenileme mağazada `U` ile aynı turda denenir.

### Trust class (A/B/C/D)

Mağaza detayında kaynak sınıfı gösterilir; **A ile B karıştırılmaz**:

| Sınıf | Anlam | Tipik `install_method` |
|-------|--------|-------------------------|
| A | Distro resmi | apt / dnf / pacman |
| B | Proje resmi ikili + imza | `download_archive` + `signature` |
| C | Kayıt defteri (risk notu) | cargo / npm / pipx / uv / brew |
| D | Manuel belgeler (veya imzasız arşiv) | `manual` / imzasız archive |

Bütünlük rozeti (`hash+sig` / `hash`) sınıf B/D arşiv satırlarında ek bilgi olarak kalır.

### API hız sınırı

`api.github.com` kimliksiz çağrılarda IP başına saatte 60 istekle sınırlıdır.
Bu nedenle:

- Çözümlemeler `manifest-cache.toml` içinde 6 saat saklanır; **taze kayıtlar
  ağa hiç çıkmaz**. Sınır korumasının asıl kaynağı bu önbellektir.
- Yayın sorgusu ayrıca `ETag` ile **koşullu** atılır (`If-None-Match`).
  `304 Not Modified` döndüğünde gövde indirilmez (bant genişliği tasarrufu);
  ancak ölçüldüğü kadarıyla `304` yanıtları da sınırdan düşer, yani tekrarlar
  ücretsiz sayılmaz.
- Checksum dosyası `github.com` üzerinden **indirilir**; bu indirmeler API
  sınırdan sayılmaz.
- `GITHUB_TOKEN` verilirse istekler kimlikli yapılır (saatte 5000).
- Sınıra takılıldığında kullanıcıya ne yapması gerektiği açıkça söylenir.

### Katmanlar

- `store::manifest` — veri modeli (`ReleaseSource`, `ResolvedRelease`),
  `ReleaseProvider` arayüzü, koşullu çözümleme sonucu
  (`ConditionalRelease`), önbellek.
- `store::github` — gerçek çözümleyici. Testlerde sahte sağlayıcı
  (`ReleaseProvider`) verilebilir; `App` alanı `Arc<dyn ReleaseProvider>`
  olduğu için sağlayıcı değiştirilebilir.
- `app` — denetim arka planda yürür (`App::check_store_updates` /
  `App::poll_store_check`), sonuç `App::store_updates` içinde tutulur ve
  mağaza detayında "yeni sürüm X → Y" olarak gösterilir. Denetim
  **mağaza sekmesinde `U`** tuşuyla tetiklenir; araç sekmesindeki `U` ise
  Nova'nın kendi güncellemesini denetler.

## Güvenlik İlkeleri

- Program ve argümanlar her zaman `std::process::Command` ile ayrı ayrı geçirilir; kabuk (`sh -c`) kullanılmaz.
- `sudo` / root istenmez, otomatik sistem değişikliği yapılmaz.
- Kurulu olmayan araçlar açıkça "kurulu değil" olarak işaretlenir; sessiz kurulum yoktur.
- Arama metni hiçbir zaman komuta dönüşmez; yalnızca filtreleme için kullanılır.
- Başlatma isteği (program, argümanlar, çalışma dizini) kullanıcı girdisinden kabuk komutu olarak birleştirilmez; `Command::new(program).args(args)` kullanılır.
- Terminal emülatörü ve araç yolları `which` ve `canonicalize` ile çözülür; aracın kendi argümanları `-e`'den sonra olduğu gibi geçirilir, emülatör tarafından yorumlanmaz.
- Başlatıcı hiçbir zaman kurulum yapmaz, `sudo` istemez veya dosya sistemi değiştirmez.
- Kullanıcı yapılandırması bozuksa **sessizce yok sayılmaz**: uygulama açılmaz, hata dosya ve satır bilgisiyle bildirilir. Böylece kullanıcı yazdığı bir aracın "neden görünmediğini" tahmin etmek zorunda kalmaz.
- `command_help`/`shell_integration_info` türlerinde araç **çalıştırılmaz**; `--help` dahi çağrılmaz. Metin katalogdan gelir.
- Mağaza kurulumu yalnızca kullanıcı alanına yapılır; hedef `$HOME` ya da Nova'nın veri dizini altında değilse güvenlik hatasıyla reddedilir.
- İndirilen arşiv **doğrulanmadan** çalıştırılmaz ve hiçbir yere yazılmaz: SHA-256 uyuşmazlığında kurulum durur.
- Arşiv çıkarımı yol geçişi, sembolik bağ ve aşırı boyut denemelerine karşı korumalıdır; çıkarma kökünün dışına çıkan hiçbir girdi kabul edilmez.
- Doğrulanabilir yayın özeti yayımlamayan uygulamalar için "Kur" eylemi sunulmaz; resmî kurulum yönergesi gösterilir.
