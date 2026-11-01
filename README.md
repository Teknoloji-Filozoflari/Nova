# Nova

<p align="center">
  <strong>Linux terminal araçlarını keşfet, düzenle ve yeni bir Konsole sekmesinde başlat.</strong>
</p>

<p align="center">
  <a href="LICENSE-MIT"><img alt="MIT lisansı" src="https://img.shields.io/badge/lisans-MIT-blue?style=flat-square"></a>
  <a href="LICENSE-APACHE"><img alt="Apache 2.0 lisansı" src="https://img.shields.io/badge/lisans-Apache--2.0-blue?style=flat-square"></a>
  <img alt="Platform: Linux" src="https://img.shields.io/badge/platform-Linux-333?style=flat-square&logo=linux">
  <img alt="Rust 2021" src="https://img.shields.io/badge/Rust-2021-orange?style=flat-square&logo=rust">
</p>

Nova, sık kullanılan komut satırı araçlarını tek bir klavye odaklı arayüzde
kategorilere ayırır. Araçları ara, favorilerine ekle, kurulum durumlarını gör
ve seçtiğini Nova'nın oturumu açık kalırken yeni bir Konsole sekmesinde
başlat.

> Nova geliştirme aşamasındadır. Hazır paket veya yayımlanmış ikili dosya
> bulunmuyor; şu anda kaynak koddan derlenir.

## İçindekiler

- [Özellikler](#özellikler)
- [Hızlı başlangıç](#hızlı-başlangıç)
- [Klavye kısayolları](#klavye-kısayolları)
- [Mağaza ve kurulum](#mağaza-ve-kurulum)
- [Dosya seçici](#dosya-seçici)
- [Yapılandırma ve kullanıcı verileri](#yapılandırma-ve-kullanıcı-verileri)
- [Güvenlik](#güvenlik)
- [Bilinen sınırlamalar](#bilinen-sınırlamalar)
- [Katkıda bulunma](#katkıda-bulunma)
- [Lisans](#lisans)

## Özellikler

- **Araç kataloğu:** AI, Git, dosya ve arama, sistem, ağ, Docker, yardımcılar
  ve özel araç kategorileri.
- **Arama ve kişiselleştirme:** araç ara, favorileri ve başlatma geçmişini
  yönet, özel araçlar ekle, proje dizinlerini kaydet.
- **Durum bilgisi:** araçların kurulu olup olmadığı <code>PATH</code> ve Nova mağaza
  dizinleri kontrol edilerek belirlenir; tespit sırasında araçlar çalıştırılmaz.
- **Mağaza:** uygulama kataloğundan ayrı bir liste. Doğrulanabilir SHA-256
  özeti olan yayınlar kullanıcı alanına kurulabilir; diğer uygulamalar için
  resmî kurulum yönergeleri gösterilir.
- **Dosya seçici:** resim veya videoyu seçip ayarladığın uygulamayla aç.
- **Klavye ve fare:** ana ekrandaki kategori ve araç listelerinde tıklama ve
  kaydırma desteği; dar terminal pencerelerine uyumlu arayüz.
- **Türkçe arayüz:** kategori adları kısa ve kolay taranır.

Nova bir terminal emülatörü veya sistem paket yöneticisi değildir. Mevcut
araçları yeniden yazmaz; onları bulmayı ve başlatmayı kolaylaştırır.

## Hızlı başlangıç

### Gereksinimler

- Linux
- Rust araç zinciri (<code>cargo</code>, <code>rustc</code>)
- Araçları yeni sekmede başlatmak için KDE Konsole (<code>konsole</code>)
- Katalogdaki kullanım metinlerini sayfalı görüntülemek için <code>less</code> veya
  <code>more</code> (isteğe bağlı)

Konsole olmadan arayüzü açıp katalogda gezebilirsin; araç başlatma isteği
Konsole bulunmadığında hata verir.

### Kaynaktan derleme

~~~~bash
git clone https://github.com/Teknoloji-Filozoflari/Nova.git
cd Nova
cargo run
~~~~

Optimizasyonlu ikili dosya oluşturmak için:

~~~~bash
cargo build --release
./target/release/nova
~~~~

## Klavye kısayolları

Ana ekrandaki temel kısayollar:

| Tuş | İşlev |
| --- | --- |
| <code>↑</code> / <code>↓</code>, <code>j</code> / <code>k</code> | Listede gezin |
| <code>←</code> / <code>→</code>, <code>h</code> / <code>l</code>, <code>Tab</code> | Paneller arasında geç |
| <code>Enter</code> | Kategoriye geç veya seçili aracı başlat |
| <code>/</code> | Ara |
| <code>f</code> | Seçili aracı favorilere ekle veya çıkar |
| <code>c</code> | Yalnızca favorileri göster veya tüm listeye dön |
| <code>r</code> | Başlatma geçmişini aç |
| <code>p</code> | Proje dizinlerini yönet |
| <code>s</code> | Ayarlar sekmesine geç (Güncelle + Tema) |
| <code>m</code> | Mağaza sekmesine geç |
| <code>u</code> | Nova'yı güncellemeyi denetle; mağaza sekmesindeyken mağaza uygulamalarını denetle |
| <code>o</code> | Dosya seçiciyi aç |
| <code>n</code> | Özel araç ekle |
| <code>d</code> | Seçili özel aracı sil |
| <code>?</code> | Yardımı göster |
| <code>Esc</code>, <code>q</code> | Ekrana göre kapat veya çık |

Onay pencerelerinde <code>Enter</code> işlemi onaylar, <code>Esc</code> vazgeçer.
Arama etkinken <code>q</code> arama metnine yazılır; <code>Esc</code> aramayı
iptal eder.

## Araç kataloğu

Varsayılan araçlar <code>assets/catalog/*.toml</code> dosyalarından (kategoriye göre) yüklenir. Katalog,
uygulama açıklamalarını, çalıştırma biçimini ve varsa güvenli kullanım
örneklerini içerir. Araçlar yeni Konsole sekmesinde başlatılır; Nova'nın
ana oturumu açık kalır.

Nova, katalog girdilerindeki kurulum durumunu çalıştırılabilir dosyayı
arayarak belirler. Ana başlatıcı kategorilerinde yalnızca kurulu araçlar
görünür; boş kategoriler gizlenir. Kurulu olmayanlar Mağaza’da kalır.
MiniMax Code için <code>mcode</code>, <code>PATH</code>'e ek olarak
Linux'taki <code>~/.minimax-code/bin/mcode</code> konumunda da aranır.

Kendi araçlarını [yapılandırma dosyasına](#yapılandırma-ve-kullanıcı-verileri)
ekleyebilir veya ana ekranda <code>n</code> tuşuyla oluşturabilirsin. Özel araçlar Nova
tarafından doğrulanıp gözden geçirme ekranında gösterildikten sonra kaydedilir.

## Mağaza ve kurulum

Mağaza, araç kataloğundan ayrı olan <code>assets/store/*.toml</code> kataloğunu kullanır.
<code>m</code> tuşuyla mağaza sekmesine geç; kategorilerde gezin, uygulama ara ve
uygulamanın kaynağını, lisansını, sürümünü ve kurulum yöntemini incele.

Mağazada iki kurulum yolu vardır:

| Yöntem | Davranış |
| --- | --- |
| Doğrulanabilir arşiv | Nova arşivi indirir, upstream'in yayımladığı SHA-256 özetiyle doğrular ve kullanıcı alanına çıkarır. |
| Elle kurulum | Nova kurulum yapmaz; resmî kurulum yönergelerini gösterir. |

Her kurulumdan önce onay istenir. Nova <code>sudo</code> veya root kullanmaz, sistem
dizinlerine yazmaz ve indirdiği betikleri kabuk üzerinden çalıştırmaz.
Kurulumlar varsayılan olarak <code>~/.local/share/nova/store/</code> altında tutulur;
<code>NOVA_STORE_DIR</code> ile mağaza dizinini değiştirebilirsin.

Mağazada <code>U</code> tuşu canlı sürüm denetimini başlatır. Sonuçlar önbelleğe alınır;
denetim kullanıcı isteğiyle çalışır, otomatik değildir. Yeni sürüm bulunursa
kurulumdan önce ayrıca onay istenir. Mağaza kataloğunun kendisi Nova ile
güncellenir; uzaktan katalog indirilmez.

## Dosya seçici

<code>o</code> tuşuyla dosya seçiciyi aç. <code>↑</code> / <code>↓</code> veya
<code>j</code> / <code>k</code> ile seçim yap; <code>→</code> / <code>l</code> dizine girer,
<code>←</code> / <code>h</code> üst dizine çıkar. <code>Enter</code> seçili resmi veya videoyu
ayarlarda belirtilen uygulamayla açar.

Resim ve video uygulamalarını <code>s</code> ayarlarından belirle. Dosya seçici dosyaları
silmez, taşımaz veya yeniden adlandırmaz. Açma işlemi kabuk kullanmadan
yürütülür; Nova yalnızca uygulama sürecinin başlatıldığını doğrulayabilir.

## Yapılandırma ve kullanıcı verileri

| Dosya | Amaç |
| --- | --- |
| <code>~/.config/nova/config.toml</code> | Elle düzenlenen araç ayarları ve özel katalog girdileri |
| <code>~/.config/nova/state.toml</code> | Nova'nın yönettiği favoriler, geçmiş, proje dizinleri ve tercihler |

<code>XDG_CONFIG_HOME</code> tanımlıysa dosyalar onun altındaki <code>nova/</code> dizinine
yerleştirilir. Nova, elle düzenlenen <code>config.toml</code> dosyasını üzerine yazmaz;
arayüzden kaydedilen durum <code>state.toml</code> içinde tutulur. Bozuk durum dosyası
silinmeden yedeklenir ve kullanıcıya uyarı gösterilir.

Örnek: var olan bir aracı favorilemek veya özel bir komut eklemek:

~~~~toml
[[tools]]
id = "btop"
favorite = true

[[tools]]
id = "sunucu-gelistirme"
name = "Geliştirme sunucusu"
category = "Özel Araçlar"
executable = "ssh"
args = ["user@host.example"]
description = "Geliştirme sunucusuna bağlanır"
launch_kind = "custom_command"
~~~~

Program ve argümanlar ayrı değerler olarak iletilir. <code>args</code> içinde kabuk
genişletmesi, boru veya yönlendirme yapılmaz. Katalog şeması ve birleştirme
kuralları için [mimari belgesine](ARCHITECTURE.md) bak.

## Güvenlik

- Nova shell komutu birleştirmez; programı ve argümanlarını ayrı ayrı başlatır.
- Araç tespiti için programları çalıştırmaz.
- Mağaza arşivlerini kurulumdan önce SHA-256 özetiyle doğrular ve güvenli
  biçimde çıkarır.
- Doğrulanabilir kurulum yöntemi olmayan mağaza uygulamalarını otomatik
  kurmaz.
- Sistem dizinlerine yazmaz ve yönetici yetkisi istemez.
- Özel araçlar mevcut kullanıcı yetkileriyle çalışır. Nova özel komutların
  güvenli olduğunu garanti etmez.

## Bilinen sınırlamalar

- Hedef platform Linux'tur; yalnızca KDE Konsole ile araç başlatma desteklenir.
- Hazır dağıtım paketi veya ikili yayın henüz yoktur. Ayarlar→Güncelle
  <code>Teknoloji-Filozoflari/Nova</code> GitHub Releases’a bakar ve uygun bir
  Linux ikilisi varsa onayla <code>~/.local/bin/nova</code> kurar; yayın yoksa
  bunu açıkça bildirir (sessiz no-op yok). Bu yol <code>git pull</code> /
  <code>cargo build</code> değildir — main’deki tema/UI (#9 vb.) için kaynak
  koddan yeniden derlemek gerekir.
- Tema tercihi (<code>varsayilan</code>, <code>acik</code>, <code>orman</code>,
  <code>amber</code>) kalıcıdır ve Ayarlar→Tema ile değişir. Varsayılan AI CLI
  tercihi de saklanır ancak başlatma akışını henüz değiştirmez.
- Fare desteği ana ekrandaki listeler içindir; ayarlar ve diğer pencerelerde
  klavye kullanılır.

## Geliştirici kontrolleri

Değişiklik göndermeden önce:

~~~~bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
~~~~

Katkı süreci için [CONTRIBUTING.md](CONTRIBUTING.md), modül sınırları ve
tasarım kararları için [ARCHITECTURE.md](ARCHITECTURE.md) dosyasını incele.
Hata veya özellik isteğini [GitHub Issues](https://github.com/Teknoloji-Filozoflari/Nova/issues)
üzerinden paylaşabilirsin.

## Lisans

Nova, tercihine göre [MIT](LICENSE-MIT) veya
[Apache License 2.0](LICENSE-APACHE) altında kullanılabilir.
