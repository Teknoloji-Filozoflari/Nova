# Nova

Nova, Linux'ta sık kullanılan terminal uygulamalarını ve komut satırı
araçlarını kategorilere ayıran, açıklayan, aratan ve seçilen aracı **yeni bir
Konsole sekmesinde** başlatmayı isteyen küçük bir TUI launcher'dır.

> **Durum:** Faz 5 (test, uyumluluk, yayın hazırlığı) tamamlandı; ancak proje
> henüz yayımlanmadı. Nova MIT veya Apache-2.0 lisanslarından biriyle
> kullanılabilir (bkz. [Lisans](#lisans)).

## Nova nedir, ne değildir

**Nedir:**

- Terminal araçlarının (opencode, btop, fzf, rg, duf…) katalogu üzerinde klavyeyle
  gezinen, arayan, favorileyen bir başlatıcı.
- Doğrulanabilir yayın özeti (SHA-256) olan araçları **kullanıcı alanına** indirip
  kuran bir mağaza.
- Kataloğu koddan ayrı tutan (TOML) ve kullanıcının kendi araçlarını eklemesine
  izin veren bir yapı.

**Değildir:**

- Araçların yerine geçmez; `btop`, `yazi`, `lazygit` gibi programları yeniden
  yazmaz, işlevlerini içine kopyalamaz.
- Sistem paket yöneticisi değildir: `sudo`/root istemez, sistem dizinlerine
  yazmaz, `apt`/`pacman`/`dnf` çağırmaz. Yalnızca kullanıcı alanına kurar.
- Güvenlik kanalı yoktur: indirilen betikleri `curl | sh` gibi kabuk üzerinden
  çalıştırmaz. Dosyayı indirir, **SHA-256 özetini doğrular**, sonra güvenli
  biçimde çıkarır.
- Genel amaçlı bir görev yöneticisi, dosya yöneticisi ya da terminal emülatörü
  değildir.
- Konsole dışındaki terminal emülatörlerini şu an kullanmaz (bkz. [Bilinen
  sınırlamalar](#bilinen-sınırlamalar)).

## Özellikler

- Sekiz kategorili, katalogdan okunan araç listesi; arama, favoriler, başlatma
  geçmişi.
- Kurulum durumu (`✓` kurulu, `✗` kurulu değil, `?` kontrol edilemedi) yalnızca
  `PATH` taranarak belirlenir; hiçbir araç tespit için çalıştırılmaz.
- `Enter` ile araç yeni Konsole sekmesinde **istenir**; Nova'nın kendi oturumu
  açık kalır.
- Araç başına çalışma dizini (`home`, `custom`, `project`); oturumluk proje
  dizini seçicisi.
- Kullanıcı yapılandırması (`config.toml`) ile gömülü kataloğun `id` bazında
  birleşmesi; Nova'nın yazdığı ayrı durum dosyası (`state.toml`).
- Nova içinden yeni özel araç ekleme (doğrulama + gözden geçirme ekranı).
- **Mağaza**: sol paneldeki sekme listesinin son satırıdır; oradan seçilince
  ekran üç sütuna döner (kategoriler | mağaza kategorileri | uygulamalar).
  Doğrulanabilir yayın özeti olan uygulamalar SHA-256 doğrulamasıyla kullanıcı
  alanına kurulur, diğerleri resmî kurulum yönergesini gösterir.
- **Canlı sürüm takibi**: mağazada `U` ile GitHub Releases denetimi; yeni
  sürümler gösterilir ve güncellenebilir.
- Dar terminal desteği (tek sütun, minimum ekranlar) ve panik durumunda
  terminali geri yükleme.

## Gereksinimler

| Gereksinim | Durum |
|------------|-------|
| Linux | Hedef platform. macOS/Windows desteklenmiyor ve denenmedi. |
| Rust araç zinciri (`cargo`, `rustc`) | Bu depoda **1.99.1** ile derlendi ve test edildi. |
| Asgari Rust sürümü | **Belirlenmedi.** `Cargo.toml` bir MSRV bildirmiyor; yalnızca Rust 1.99.1 ile derleme ve test yapıldı. |
| `konsole` (KDE Konsole) | Yeni sekmede başlatma için gerekli. |
| `less` (yoksa `more`) | Yalnızca `command_help` türündeki araçların metnini göstermek için. |

`konsole` yoksa Nova açılır ve gezinmeyi destekler; yalnızca başlatma istekleri
`Desteklenen bir terminal emülatörü bulunamadı (aranan: konsole)…` hatasıyla
reddedilir.

## Derleme ve çalıştırma

```bash
cargo build            # debug
cargo run              # derleyip çalıştır

cargo build --release  # ./target/release/nova
```

Doğrulama komutları (CI iş akışı bu üçünü tanımlar):

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Son doğrulama sonuçları bu belgenin proje sahibi tarafından güncellenir; test
sayısı ve sonuçları için ilgili faz raporuna bakın.

### Dağıtıma özgü notlar

Bu bölüm her dağıtım için **ayrı** tutulur; genel adımlar yukarıdadır.
Aşağıdaki paket adları ve komutlar bu projede **doğrulanmadı** (yalnızca hedef
dağıtımlar olarak listelenmiştir, denenmemiş komut yazılmaz):

- **CachyOS (Arch tabanlı):** hedef. Paket adları doğrulanmadı.
- **Debian 13:** hedef. Paket adları doğrulanmadı.
- **Pardus (Debian tabanlı):** hedef. Paket adları doğrulanmadı.

Nova'da dağıtıma özgü kod yolu veya hack **yoktur**; varsayımlar yalnızca POSIX
araçları (`$HOME`, `PATH`, `konsole`, `less`/`more`) ve KDE Konsole'dur.

## Klavye kısayolları

| Tuş | İşlev |
|-----|-------|
| `Esc` | Yardım açıksa kapatır → aramadaysa aramayı iptal eder → hiçbiri yoksa çıkar |
| `q` | Çıkar (arama modunda `q` harf olarak yazılır) |
| `↑`/`↓` veya `j`/`k` | Seçimde gezinir; bekleyen durum mesajını temizler |
| `←`/`→` veya `h`/`l` | Kategori / araç paneli arasında geçiş |
| `Tab` | Kategori panelindeyse araç paneline geçer, araç panelindeyse seçili aracı başlatır |
| `Enter` | Aynı davranış: kategorideyse araç paneline geçer, araçtaysa başlatır |
| `/` | Arama başlatır (araç paneli odaklanır) |
| `f`/`F` | Seçili aracı favorilere ekler/çıkarır (yalnızca araç panelinde) |
| `c`/`C` | Yalnızca favorileri gösterir |
| `r`/`R` | Başlatma geçmişini gösterir (içinde `X` + onay temizler) |
| `p`/`P` | Proje dizini seçicisi (içinde `A` ekle, `D` listeden çıkar, `Enter` etkinleştir) |
| `s`/`S` | Varsayılan terminali ve çalışma dizinini düzenler; dosya yollarını gösterir |
| `m`/`M` | Mağaza sekmesine atlar (kısayol; asıl yol sol paneldeki "Mağaza" satırı) — bkz. [Mağaza](#mağaza) |
| `u`/`U` | Nova güncellemelerini denetler; mağaza sekmesindeyken mağaza uygulamalarının canlı sürümlerini denetler |
| `o`/`O` | Dosya seçici (seçilen dosya medya uygulamasıyla açılır) |
| `n`/`N` | Yeni özel araç formu |
| `d`/`D` | Özel aracı siler (onay ister) |
| `?` | Klavye yardımı, dosya yolları ve etkin çalışma dizini |

Modal açıkken ana ekran tuşları çalışmaz: `n`, `d`, `c`, `q` gibi harfler metin
alanına yazılır veya yok sayılır. Onay ekranlarında `Enter` onaylar, `Esc` ve
`n` vazgeçer.

Ayarlar ekranında terminal olarak `konsole` seçilebilir veya alan boş bırakılarak
varsayılan kullanılabilir. Çalışma dizini mutlak yol, göreli yol ya da `~/...`
biçiminde girilebilir; kayıt öncesi mevcut bir dizin olduğu doğrulanır. Nova
eksik dizin oluşturmaz. Ayarlar ve katalog dosyalarının tam yolları aynı
ekranda gösterilir. Yalnızca Konsole desteklendiği için başka terminal adları
kabul edilmez.

Dar terminalde (70 sütundan dar) kategori paneli gizlenir; `h`/`←` odak
değiştirmez, durum satırı terminali genişletmeyi önerir.

## Konsole entegrasyonu

Nova, seçilen aracı şu biçimde yeni bir sekmede açar:

```
konsole --new-tab --workdir <dizin> -e <program> <argümanlar…>
```

- Program ve argümanlar **ayrı ayrı** verilir (`std::process::Command`); kabuk
  yoktur, tek bir komut dizesi birleştirilmez.
- Konsole süreci **ayrı** başlatılır; tüm stdio `null`'a bağlanır. Nova'nın
  terminali bundan etkilenmez.
- Çalışma dizini Nova tarafından doğrulanır (`--workdir` geçersizse Konsole
  sessizce ev dizinine döner); geçersizse Nova hiçbir şey göndermez.
- Durum satırı `… başlatılması istendi …` der. Bu **yalnızca isteğin
  gönderildiğini** bildirir; sekmenin açıldığı Nova tarafından doğrulanamaz.

Yalnızca `terminal` çalıştırma biçimi (yeni sekme) uygulanır. `background` /
`detached` türündeki girdiler bu sürümde reddedilir. Başka bir emülatör eklemek
için `src/launcher/terminal.rs` içindeki adapter arayüzüne yeni bir adapter
yazılması gerekir (mimari bunu açık bırakır; bugün yalnızca Konsole vardır).

### Doğrulanmış / doğrulanmamış

Bu depoda **gerçek ortamda** (Arch/CachyOS, Konsole 26.08.2, Wayland, PTY
üzerinden) gözlemlenenler:

| Senaryo | Sonuç | Yöntem |
|---------|-------|--------|
| Kurulu bir TUI aracı için başlatma isteği | Konsole süreci başlatıldı, Nova açık kaldı, `q` ile normal çıkış | Gerçek `konsole` |
| Proje diziniyle başlatma komutu | `konsole --new-tab --workdir <proje> -e <program>` (bayrakla kaydeden sahte `konsole` ile argüman kaydı) | Sahte `konsole` |
| Eksik komut | Hiçbir süreç başlatılmadan açık hata | PTY |
| `konsole` yok (`PATH`'te yok) | Açık hata, Nova açık kalır, `q` ile çıkış | PTY |
| Bozuk `config.toml` | Alt ekrana girmeden hata, çıkış kodu 1 | PTY |
| Terminal penceresi kapanması | Nova asılı kalmaz | PTY |

**Doğrulanmamış** (iddia edilmez): sekmenin gerçekten açıldığı ve aracın içinde
çalıştığı; Debian 13 ve Pardus'ta derleme; asgari Rust sürümü; herhangi bir
paket adı.

## Katalog ve kullanıcı yapılandırması

İki ayrı dosya vardır:

| Dosya | Sahibi | İçerik |
|-------|--------|--------|
| `assets/catalog.toml` (ikiliye gömülü) | Uygulama | Varsayılan katalog; güncellemeyle değişebilir |
| `$XDG_CONFIG_HOME/nova/config.toml` (isteğe bağlı; yoksa `~/.config/nova/config.toml`) | **Sen** (elle) | Araç tanımları, `[ui]` ayarları |
| `$XDG_CONFIG_HOME/nova/state.toml` (yoksa `~/.config/nova/state.toml`) | **Nova** | Favoriler, geçmiş, gizlenen araçlar, formla eklenen araçlar, proje dizinleri, `[prefs]` |

`XDG_CONFIG_HOME` yalnızca mutlak ve boş olmayan bir yol olduğunda kullanılır;
tanımsız, boş veya göreli ise Nova `$HOME/.config/nova/` konumuna döner.

Nova, elle yazdığın `config.toml` dosyasını **yeniden yazmaz**. Arayüzden
yapılan değişiklikler yalnızca `state.toml` dosyasına gider.

### Birleştirme kuralları

- İki kaynak `id` alanına göre birleşir: aynı `id` her ikisinde varsa dolu
  kullanıcı alanı kazanır, boş alanlar varsayılandan gelir.
- Yalnızca kullanıcı dosyasında olan `id` yeni araç olarak eklenir
  (`Özel Araçlar` kategorisi bunun içindir).
- Aynı `id` kullanıcı dosyasında iki kez geçerse açık hata verilir.

Örnek `config.toml`:

```toml
[ui]
recent_limit = 20        # 0 = varsayılan (20); 100 üzeri 100'e kırpılır

[[tools]]
id = "btop"              # mevcut aracı favori yap; diğer alanlar varsayılandan
favorite = true

[[tools]]
id = "sunucu-uretim"
name = "Üretim sunucusu"
category = "Docker ve Sunucu"
executable = "ssh"
args = ["user@host.example"]
description = "Üretim sunucusuna bağlanır"
launch_kind = "custom_command"

[[tools]]
id = "opencode"
working_directory = "project"
working_dir_path = "/path/to/project"
```

### Doğrulama kuralları

- `category` yalnızca şu sekiz değerden biri olabilir (bilinmeyen değerde
  geçerli liste mesajda verilir): `AI ve Kodlama`, `Git ve Geliştirici
  Araçları`, `Dosya Yönetimi ve Arama`, `Sistem ve Kaynak Kullanımı`,
  `Ağ ve Bağlantı`, `Docker ve Sunucu`, `Günlük Terminal Yardımcıları`,
  `Özel Araçlar`.
- `executable` bir **program adıdır** (`ssh`); yol (`/usr/bin/ssh`) reddedilir.
- `args` öğeleri boş olmayan ayrı argümanlardır; `|`, `>`, `$VAR` genişletilmez.
- `launch_kind = "command_help"` için `help_text` zorunludur.
- `custom` / `project` çalışma dizini için `working_dir_path` zorunludur.

Bozuk `config.toml` sessizce yok sayılmaz: Nova alt ekrana geçmeden, dosya adı
ve satır/sütun içeren bir hata yazar ve çıkış kodu 1 ile biter.

Cloudflare örneği `config.toml` içinden özelleştirilebilir. Aynı `id` ile
`executable` ve `args` alanlarını vererek programı, veri miktarını veya süre
sınırını değiştirebilirsin. Varsayılan komut `curl` ile 25 MB indirir ve
ortalama indirme hızını yazar.

### Başlatma türleri (`launch_kind`)

| Değer | Davranış | Örnek |
|-------|----------|-------|
| `interactive` (varsayılan) | Araç olduğu gibi yeni sekmede başlatılır | opencode, btop, fzf |
| `command_help` | Araç **çalıştırılmaz**; katalogdaki sabit `help_text` sayfalayıcıda gösterilir | rg, fd, jq, curl |
| `shell_integration_info` | Araç çalıştırılmaz; kabuk/git entegrasyonu bilgisi gösterilir | atuin, delta |
| `custom_command` | Katalogdaki `executable` + `args` çalıştırılır | SSH profilleri |

`help_text` asla `--help` çalıştırılarak üretilmez. `command_help` /
`shell_integration_info` metinleri geçici bir dosyaya yazılıp sayfalayıcıya
(`less`, yoksa `more`) geçirilir. Her istek için geçici dizinde özel bir
`0700` klasör ve içinde `0600` izinli `help.txt` dosyası oluşturulur; mevcut
dosyalar takip edilmez.
Konsole isteği asenkron işlediğinden, Nova'nın dahili sayfalayıcı modu metni
gösterir ve sayfalayıcı kapandıktan sonra dosyayı siler. Konsole dahili modu
başlatamazsa artık dosya ve klasör geçici dizinde kalabilir.

### Çalışma dizini

| Değer | Anlamı |
|-------|--------|
| `current` (veya boş) | Ev dizini (`$HOME`) |
| `home` | Ev dizini |
| `custom` / `project` | `working_dir_path` içindeki dizin. Oturumda etkin bir proje dizini varsa `project` için o dizin kullanılır. |

Yol mutlak ve var olan bir dizin olmalıdır; değilse başlatma reddedilir. Nova
asla sessizce ev dizinine düşmez.

### `state.toml`

```toml
favorites = ["curl", "btop"]

[[recents]]                # en yeni önce, en fazla recent_limit kayıt
id = "btop"
name = "btop"
at = 1761750000            # UNIX saniye
status = "request_sent"    # veya "rejected"

hidden_tools = ["sshd-baglantim"]
project_dirs = ["/path/to/project"]

[prefs]
recent_limit = 20
# default_working_dir = "/path/to/workspace"
# preferred_terminal = "konsole"
```

- **Yazma güvenliği:** aynı dizinde benzersiz bir geçici dosya `0600` izinleri
  ve çakışmayı önleyen oluşturma ile açılır, `sync_all` ile diske indirilir,
  sonra `state.toml` adına yeniden adlandırılır. Var olan `config.toml`
  değiştirilmez.
- **Bozuk dosya:** silinmez; `state.toml.bozuk-<unix>` olarak yedeklenir ve
  uygulama boş durumla açılır. Tek bir bozuk özel araç kaydı yalnızca kendisini
  düşürür.
- **Favoriler** `id` ile saklanır; katalogdan kalkan aracın kimliği listede
  kalır.
- **Geçmiş** Enter ile yapılan başlatma denemelerini yazar. `request_sent`,
  terminale isteğin gönderildiğini; `rejected` ise denemenin reddedildiğini
  belirtir. İstek gönderilmiş olması harici uygulamanın sağlıklı çalıştığını
  kanıtlamaz. Eski kayıtlarda durum alanı yoksa `request_sent` kabul edilir.
- **Proje dizinleri** kalıcıdır, etkin seçim oturumluktur; yol oluşturulmaz.

## Özel araçlar

`n` ile açılan form: görünen ad, açıklama, kategori, program, argümanlar, çalışma
dizini ve çalıştırma türü. `←`/`→` ile `etkileşimli` veya `özel komut` seçilir.
`Enter` doğrular; doğruysa gözden geçirme ekranında kaydedilecek tam
kayıt gösterilir, `Enter` ile `state.toml` dosyasına yazılır, `Esc` ile forma
dönülür.

- Ad ve program boş olamaz; program boşluk veya `/` içeremez.
- Argümanlar yalnızca boşlukla ayrılır; tırnak, `*`, `$HOME` ve boru
  yorumlanmaz, olduğu gibi tek argüman olarak geçer.
- Çalışma dizini boş bırakılırsa ev dizini kullanılır.
- `etkileşimli` tür bu formda argüman kabul etmez; `özel komut` argümanları
  kabuksuz ve ayrı ayrı geçirir. Kullanım metni ve kabuk entegrasyonu bilgi
  türleri ek metin gerektirdiği için `config.toml` içinde tanımlanır.
- Gözden geçirme ekranı çalışacak programı, argümanları, dizini ve türü gösterir.
  Özel program Nova tarafından doğrulanmaz ve kendi kullanıcı yetkilerinle
  çalışır; Nova shell çalıştırmaz, programı kurmaz veya güncellemez.
- Var olan özel araç düzenlenemez (silip yeniden eklenir).
- `config.toml` araçları Nova'da "silinince" dosyadan silinmez, `hidden_tools`
  içine eklenir.

## Mağaza

Mağaza, uygulama kataloğundan **ayrı** bir listedir (`assets/store.toml`) ve
ayrı bir ekran (modal) değildir: **sol paneldeki sekme listesinin son satırıdır.**
Sekme seçildiğinde ekran üç sütuna döner:

```
┌ Kategoriler ────────┐ ┌ Mağaza Kategorileri ┐ ┌ Dosya ve Arama ──────┐
│▶ AI                 │ │▶ AI            (3) │ │ ▶ ✓ ripgrep (rg)     │
│  Git                │ │  Git           (1) │ │   ✓ fzf              │
│  …                  │ │  Dosya ve Arama(4)│ │   · eza              │
│  Mağaza (19)        │ │  Sistem       (2) │ │   · bat              │
└─────────────────────┘ └───────────────────┘ └──────────────────────┘
```

- **Sol sütun** her zaman sekme listesidir: sekiz araç kategorisi ve en altta
  "Mağaza".
- **Orta sütun** mağaza kategorilerini, **sağ sütun** o kategorideki
  uygulamaları gösterir. Altta mağaza detayı yer alır (sürüm, kurulum durumu,
  canlı takip, kaynak, lisans).
- `m` tuşu doğrudan mağaza sekmesine atlar (kısayoldur; asıl yol sekme
  listesidir). `h`/`l` (ya da `←`/`→`) paneller arasında gezinir, `↑`/`↓`
  seçimi taşır.
- Dar terminalde üç sütun yan yana sığmaz; mağaza kategorileri üste,
  uygulamalar alta yerleşir.

Uygulamalar kategoriye ve ada göre aranabilir (`/`); detay ekranında ne işe
yaradığı, kaynağı, lisansı, sürümü, kurulu olup olmadığı ve desteklenen kurulum
yöntemi gösterilir.

### İki kurulum yöntemi

| Yöntem | Anlamı | Nova ne yapar |
|--------|--------|---------------|
| `download_archive` | Upstream doğrulanabilir bir yayın özeti yayımlıyor | İndirir, **SHA-256 özetini doğrular**, güvenle çıkarır, kullanıcı alanına kurar |
| `manual` | Upstream doğrulanabilir özet yayımlamıyor | Kurulum yapmaz; resmî kurulum yönergesini gösterir |

`download_archive` kaydı **mutlaka** `download_url` ve `checksum` içermelidir;
eksik bir kayıt katalog yüklenirken reddedilir. Böylece "Kur" düğmesi yalnızca
gerçekten doğrulanabilir bir indirme için görünür.

### Şu anda indirilebilir olan uygulamalar

| Uygulama | Sürüm | Platform |
|----------|-------|----------|
| [ripgrep](https://github.com/BurntSushi/ripgrep) | 15.2.0 | linux-x64 (musl) |
| [fzf](https://github.com/junegunn/fzf) | 0.74.5 | linux-x64 |
| [eza](https://github.com/eza-community/eza) | 0.23.5 | linux-x64 (musl) |
| [lazygit](https://github.com/jesseduffield/lazygit) | 0.66.2 | linux-x64 |
| [lazydocker](https://github.com/jesseduffield/lazydocker) | 0.25.2 | linux-x64 |
| [duf](https://github.com/muesli/duf) | 0.9.1 | linux-x64 |

Özetler upstream'in kendi yayın checksum dosyalarından alınmıştır
(ripgrep/fzf için `SHA256SUMS`, lazygit/lazydocker/duf için `checksums.txt`,
eza için yayın notları) ve Nova tarafından **indirilen dosyanın özetiyle
karşılaştırılarak** doğrulanır.

### Kurulum akışı

1. Mağazada uygulama seçilir, `Enter` ile **onay** istenir (uygulama, sürüm,
   kaynak, hedef dizin ve yapılacak işlemler gösterilir).
2. Arşiv indirilir. İndirme arka planda yürür; Nova'nın ana döngüsü
   kilitlenmez, durum satırında ilerleme görünür.
3. **SHA-256 özeti doğrulanır.** Uyuşmazlıkta kurulum durur ve **hiçbir dosya
   yazılmaz**.
4. Arşiv geçici bir dizine çıkarılır: mutlak yollar ve `..` bileşenleri
   reddedilir, sembolik bağ oluşturulmaz, girdi sayısı ve toplam boyut
   sınırlıdır.
5. Çalıştırılabilir dosya bulunur ve `~/.local/share/nova/store/<id>/bin/`
   altına `0755` izniyle yerleştirilir. Geçici dosyalar silinir.

Kurulum tamamen kullanıcı alanındadır: `sudo`/root yok, sistem dizinlerine
yazılmaz, sistem paket yöneticisi çağrılmaz, indirilen betik kabuk üzerinden
çalıştırılmaz. `NOVA_STORE_DIR` ortam değişkeniyle mağaza dizini
taşınabilir/taşınabilir-kurulum için değiştirilebilir.

Mağazadan kurulan uygulama, PATH'te olmasa bile Nova'da "kurulu" görünür:
tespit sırasında mağazanın `bin` dizinleri de taranır.

### Canlı sürüm takibi

Mağaza sekmesinde `U` tuşu **canlı sürüm denetimi** yapar: izlenen her uygulamanın
GitHub deposundaki en güncel yayını sorgulanır ve yeni sürüm varsa mağaza
detayında "yeni sürüm X → Y" olarak gösterilir. `Enter` ile güncelleme
onaylanır.

Bu, "uzaktan gelen katalog" **değildir**. Güvenlik modeli şöyledir:

| Ne | Nerden gelir | Değiştirilebilir mi |
|-----|--------------|---------------------|
| Hangi depo izlenecek | Gömülü `assets/store.toml` | Hayır — kullanıcı değiştiremez, ağ da değiştiremez |
| Arşiv dosyasının adı | Gömülü katalog | Hayır |
| En güncel sürüm | GitHub Releases API | Yalnızca ipucu |
| SHA-256 özeti | **Upstream'in kendi** checksum dosyası (ya da yayın notları) | Hayır |

Yani ağdan gelen veri yalnızca "şu sürümü kontrol et" demektir; çalıştırılacak
dosyanın bütünlüğü upstream'in yayımladığı özetle korunur. Upstream özeti
değiştirilmeden tek bir bayt bile kurulamaz.

Mağaza kataloğu ise **gömülü kalır**; Nova'nın kendi deposundan uzaktan
okunmaz. Yeni bir mağaza uygulaması eklemek Nova'nın güncellenmesiyle olur,
sürüm numarasının tazeliği ise `U` tuşu ve önbellek döngüsüyle sağlanır. Bu,
bütünlüğün tek bir güvenlik sınırından (upstream özeti) geçmesini korumak için
bilinçli bir tercihtir.

Denetim sonuçları `~/.local/share/nova/store/manifest-cache.toml` dosyasında
6 saat saklanır; önbellek tazeyse API'ye hiç gidilmez — hız sınırı koruması
buradan gelir. Tekrarlı denetimler ayrıca `ETag` ile **koşullu** yapılır ve
`304 Not Modified` döndüğünde gövde indirilmez; ancak `304` yanıtları da
sınırdan düştüğü için tekrarlar ücretsiz sayılmaz.

GitHub'ın kimliksiz API sınırı IP başına saatte 60 istektir. Sınıra takılırsanız
`GITHUB_TOKEN` ortam değişkenini ayarlayarak kimlikli isteklerle (saatte 5000)
sınırı yükseltebilirsiniz; Nova bu değişkeni kendiliğinden kullanır.

## Güvenlik modeli

- Program ve argümanlar **ayrı ayrı** geçirilir; kullanıcı girdisinden kabuk
  komutu birleştirilmez. Kabuk (`sh -c`) hiç çalıştırılmaz.
- `sudo` kullanılmaz, sistem dosyaları değiştirilmez.
- Mağaza kurulumu **yalnızca kullanıcı alanına** yapılır; hedef dizin
  `$HOME` altında ya da Nova'nın kendi veri dizininde olmalıdır, aksi hâlde
  güvenlik hatasıyla reddedilir.
- İndirilen arşiv **doğrulanmadan** çalıştırılmaz ve hiçbir yere yazılmaz.
- Arşiv çıkarımı yol geçişi (path traversal), sembolik bağ ve aşırı boyut
  denemelerine karşı korumalıdır.
- Katalogdaki sabit metinler dışında hiçbir program Nova tarafından
  "bilgi toplamak için" çalıştırılmaz.
- Kalıcı kullanıcı verisi `$XDG_CONFIG_HOME/nova/state.toml` dosyasına (değişken
  yoksa `~/.config/nova/state.toml`) özel izinlerle yazılır. Yardım metni için
  Nova ayrıca `0600` izinli geçici dosya oluşturur;
  dahili sayfalayıcı bunu kapatınca dosyayı siler. Kullanıcı onayıyla eksik
  çalışma dizini de oluşturulabilir.

**Sınır:** Nova, kullanıcının yazdığı programın güvenli olduğunu kanıtlamaz;
o program senin kullanıcı yetkilerinle çalışır. Sekme açıldıktan sonra ne
olduğu Nova'nın denetiminde değildir. Aynı şekilde mağaza, upstream
checksum'ının **doğru** olduğunu varsayar; Nova yalnızca indirilen dosyanın
o özetle birebir eşleştiğini doğrular.

## Bilinen sınırlamalar

- Yalnızca **Konsole** desteklenir; kitty/alacritty/gnome-terminal vb. kurulu
  olsa bile `konsole` aranır ve yoksa başlatma reddedilir.
- Sekmenin açıldığı **doğrulanmaz**; mesaj "istendi" der.
- `background` / `detached` başlatma yoktur.
- Mağaza kataloğu **canlı takip edilen** uygulamalar için GitHub Releases
  üzerinden denetlenebilir; ancak denetim **kullanıcı isteğine bağlıdır**,
  otomatik olarak yapılmaz. Denetim 6 saatlik önbellekle sınırlıdır.
- GitHub'ın kimliksiz API sınırı IP başına saatte 60 istektir; sınıra
  takılırsanız `GITHUB_TOKEN` ayarlayın (Nova bunu kendiliğinden kullanır).
- Mağazadaki 13 uygulama `manual` yöntemdedir (upstream doğrulanabilir özet
  yayımlamıyor); onlar için resmî yönerge gösterilir, kurulum yapılmaz.
- `config.toml` içindeki `[ui] theme` alanı okunur ama **henüz uygulanmaz**
  (tek tema vardır).
- **Asgari Rust sürümü doğrulanmadı** (bkz. Gereksinimler); yalnızca 1.99.1
  ile test edildi.
- Terminal penceresi Nova çalışırken kapatılırsa Nova beklemeden çıkar, ancak
  alt ekrandan çıkış dizisi yazılamaz (tty yok); bu bir çökme/panik değildir.
- Konsole dahili sayfalayıcıyı başlatamazsa `command_help` geçici dosyası ve
  özel klasörü geçici dizinde kalabilir; dosya `0600`, klasör `0700` olur.
- `Özel Araçlar` varsayılan katalogda boştur; araçlar `config.toml` veya Nova
  içinden eklenir.
- Cloudflare örneği en fazla 25 MB indirir; `config.toml` üzerinden komut ve
  veri miktarı değiştirilebilir.
- Özel araç formu tek satır kabul eder; boşluk içeren yol yazılamaz.
- Arama alaka sıralaması ve fare desteği yoktur.
- Dağıtım paketi, crates.io/AUR yayını ve ikili dağıtımı **henüz yoktur**.
- Güvenlik denetimi (`cargo-audit` vb.) bu ortamda çalıştırılmadı.

## Katkı

- Ayrıntılar: [CONTRIBUTING.md](CONTRIBUTING.md).
- Değişiklikten önce [ARCHITECTURE.md](ARCHITECTURE.md) dosyasındaki modül
  sınırlarını ve tasarım kararlarını okuyun.
- Katkı göndermeden önce: `cargo fmt --check`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `cargo test`.
- Yeni bağımlılık eklerken gerekçesini ve MSRV etkisini yazın.
- Hata ve özellik bildirimi için `.github/ISSUE_TEMPLATE/`; katkı açıklaması
  için `.github/PULL_REQUEST_TEMPLATE.md` kullanılabilir.
- Katkı göndererek katkınızın Nova'nın [MIT](LICENSE-MIT) ve
  [Apache-2.0](LICENSE-APACHE) lisansları altında dağıtılmasına izin vermiş
  olursunuz; ayrıntılar için [CONTRIBUTING.md](CONTRIBUTING.md) dosyasına bakın.

## Lisans

Nova çift lisanslıdır: kullanıcılar [MIT](LICENSE-MIT) veya
[Apache-2.0](LICENSE-APACHE) koşullarından birini seçebilir. Cargo metadata'sı
`MIT OR Apache-2.0` olarak tanımlıdır. Ayrıntılar için [LICENSE.md](LICENSE.md)
dosyasına bakın.
