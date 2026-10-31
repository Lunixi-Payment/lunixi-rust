# Lunixi Rust SDK

Güncel kaynak sürümü: **0.1.2**. Aşağıdaki önceki **0.1.1** kayıt deposu sürümü doğrulanmış yayındır; yeni kaynak yamasının orada yayımlandığı iddia edilmez.

Yeni kaynak yaması mağaza sitesi için somut bir sunum örneği ekler. **Varsayılan gömülü ödemedir**: yalnız resmi Lunixi bundle yüklenir ve mevcut form mount edilir; yöntem ve çoklu kart seçenekleri Lunixi panelinde kalır. İsteğe bağlı yönlendirme yalnız backend `paymentPageUrl` adresini, tam HTTPS `pay.lunixi.com/payment/` doğrulamasından sonra kullanır. Adresin token’ı gömülü token’dan farklı olabilir. Eksik adres gömülü ödemeyi engellemez; yönlendirme aynı deneme/anahtarla inceleme veya uzlaştırmada kalır. Tarayıcı token’ından adres üretmeyin. Formdan önce müşteri/sipariş sahipliğini ve yerel tarayıcı CSRF’yi doğrulayın; `Cache-Control: private, no-store` gönderin, mağazanın CSP nonce değerini koruyun; token, HTML ve yönlendirme adreslerini loglamayın. Genel yanıt verisi `paymentPageUrl` alanını korur; yeni tipli ürün eşitliği iddiası yoktur.

[English README](../README.md) · [Rust dokümantasyonu](https://docs.lunixi.com/tr/sdks/rust)

Tokio üzerinde çalışan, Rust 1.89+ (edition 2024) sunucu SDK'sıdır. Axum, Actix Web ve diğer Tokio uygulamalarında kullanılabilir.

## crates.io üzerinden kurulum

[Lunixi 0.1.1 crates.io'da yayımlandı](https://crates.io/crates/lunixi/0.1.1). Cargo ile ekleyin:

```sh
cargo add lunixi@0.1.1
```

Alternatif olarak uygulamanızın `Cargo.toml` dosyasını düzenleyin:

```toml
[dependencies]
lunixi = "0.1.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Kaynaktan kurulum için registry bağımlılığı yerine GitHub sürüm etiketi kullanılabilir:

```toml
[dependencies]
lunixi = { git = "https://github.com/Lunixi-Payment/lunixi-rust", tag = "v0.1.1" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
let mut config = lunixi::Config::new(
    "https://api-gateway.lunixi.com",
    std::env::var("LUNIXI_KEY_ID")?,
    std::env::var("LUNIXI_PRIVATE_KEY_PEM")?,
);
config.max_retries = 2;
let client = lunixi::Client::new(config)?;
let intent = lunixi::IntentRequest::new(10050,"TRY","ORD-1001","https://merchant.example/payment/return");
let response = client.payments().create_intent(&intent,&kayitli_deneme_anahtari).await?;
```

Merchant başına bir istemci oluşturup klonlarını paylaşın; token önbelleği ve bağlantı havuzu ortaktır. Ed25519 PKCS8 PEM/base64 DER/32 bayt seed/doğrulanmış 64 bayt keypair kabul edilir. Açık anahtar panelde kaydedilir, özel anahtar sunucuda kalır. TEST/LIVE seçimini merchant anahtarı belirler. HTTPS zorunludur; yönlendirmeler, otomatik reqwest tekrarları ve ortam proxy'leri kapalıdır.

`amount` ve sepet `price` alanları tam sayı kuruştur; `10050` = 100,50 TRY. Sepet toplamı intent tutarıyla aynı olmalıdır. Kaldırılmış kök alanlar `price`, `paidPrice`, `autoCapture` yerelde reddedilir. Taksit/vade farkını Lunixi formu yönetir. Token yalnız ilgili siparişin yetkili müşterisine `Cache-Control: no-store` ile verilir ve mevcut Lunixi formu açılır. Token/PAN/CVV loglanmaz; varsayılan örnekler ham kart toplamaz.

## Kapsam ve güvenli tekrar

- `payments()`: intent, sorgulama, liste, yöntemler, capture/refund/void, split ve split iadesi.
- `fraud()`: değerlendirme, olay gönderme ve karar sorguları.
- `send(Request)`: diğer belgelenmiş `/api/v1/` merchant uçları için ortak imzalı taşıma. 0.1.1 sürümü sigorta/cüzdan/banka/pazaryeri vb. ürünlerde tipli yardımcı eşitliği iddia etmez.
- `WebhookVerifier` ve isteğe bağlı `SqliteInbox`: ham v2 doğrulama ve kalıcı transaction içinde tekrar koruması.

Finansal mutasyonlar 16–128 karakterlik boşluksuz yazdırılabilir ASCII anahtar ister. Anahtar işlemden önce kaydedilir; tekrar aynı gövde ve anahtarla gider. Varsayılan tekrar sıfırdır; yalnız GET/anahtarlı istekler yapılandırılan sınırda tekrarlanır. Anahtarsız mutasyon kör tekrarlanmaz. Belirsiz sonucu yeni ödeme başlatmadan önce uzlaştırın.

HTTP başarısı, token, tarayıcı dönüşü veya `AUTHORIZED` tahsilat değildir. Merchant/sipariş/ödeme kimliği, beklenen tutar/para birimi ve nihai tahsilat/uzlaştırma durumunu doğrulamadan siparişi tamamlamayın. Split bekleme durumunda HTTP 400 içeren `Response` dönebilir; ödenmiş sayılmaz. Yanıt zarfı, `data`, `page_info` ve ikili `bytes` korunur. Hatalarda uzak sunucunun gövdesi/mesajı/URL ve secret yoktur; veri erişimcileri yine müşteri bilgisi içerebilir ve loglanmamalıdır.

## Webhook ve kalıcı tekrar koruması

```rust
let verifier = lunixi::WebhookVerifier::new(vec![std::env::var("LUNIXI_WEBHOOK_SECRET")?])?;
let event = verifier.verify(ham_govde,basliklar)?;
```

Başlık tipi `reqwest::header::HeaderMap` (HTTP 1.x) şeklindedir. Ham baytları JSON middleware'den önce alın; yinelenen kimlik başlıklarını birleştirerek gizlemeyin. HMAC, event ID + zaman damgasının birebir metni + ham gövdeye bağlıdır; sabit zamanlı doğrulanır. Tolerans varsayılan 5 dakika, en fazla 15 dakikadır; kapatılamaz. İmzalı gövdede `type` gerekir; ID/tür uyuşmazlığı ve yinelenen başlık/JSON anahtarı reddedilir. Rotasyon için en fazla üç secret verilebilir.

`sqlite` özelliğini açarak kalıcı inbox kullanın:

```rust
let inbox = lunixi::SqliteInbox::new(merchant_scope)?;
lunixi::SqliteInbox::ensure_schema(&connection)?;
let uygulandi = inbox.process(&mut connection,&event,|transaction| {
    dogrulanmis_siparisi_isle(transaction,&event)
})?;
```

`dogrulanmis_siparisi_isle`, finansal alanları eşleyip verilen transaction ile sipariş/outbox durumunu uygulayan kendi adaptörünüzdür. Olay kaydı ve iş durumu aynı transaction içinde commit edilir. Callback başarısızsa marker da geri alınır; yeniden teslim güvenle işlenir. Tekrar callback'i çalıştırmaz; aynı ID farklı gövdeyle gelirse reddedilir. Merchant başına scope ayırın ve tekrar kayıtlarını replay politikanız boyunca saklayın. Harici yan etkiler outbox worker'a aittir. SQLite işi senkron olduğundan asenkron sunucuda blocking worker kullanın; kalıcı depolama hatasına HTTP 2xx dönmeyin.

[Kalıcı bildirim örneği](../examples/durable_webhook.rs), stdin'den gelen imzalı teslimi doğrulayıp yalnız event ID/türünü kaydeder; siparişi tamamlamaz. SQLite özelliği C derleyicisi ister. Eski olay için zaman kontrolünü kapatmak yerine sağlayıcıdan yeni replay isteyin.

```sh
cargo fmt --all --check
cargo test --locked --all-targets --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo audit
cargo deny --all-features check advisories licenses sources
cargo package --locked --allow-dirty --all-features
```

Testler gerçek yerel TLS, sentetik Node.js imza vektörleri, eşzamanlı token edinimi ve SQLite geri alma/yeniden başlatma/tekrar senaryolarını kapsar. Gerçek merchant çağrısı gerekmez. [Güvenlik](../SECURITY.md) · [Katkı](../CONTRIBUTING.md) · [MIT](../LICENSE)

## Mağaza içi entegrasyon örneği

Kaynak checkout örneği [checkout_presentation.rs](https://github.com/Lunixi-Payment/lunixi-rust/blob/main/examples/support/checkout_presentation.rs) dosyasını kullanır: `render_checkout_html(token, nonce)` resmi formu mount eder; `hosted_checkout_url(paymentPageUrl)` isteğe bağlı adresi doğrular. Kendi yetkili ve CSRF korumalı mağaza controller’ınıza uyarlayın. `LUNIXI_CHECKOUT_MODE` varsayılanı `embedded`, alternatif değer `hosted` olur. CLI üretilen HTML/token/URL verisini yazdırmaz. Bunlar kaynak örnek yardımcılarıdır; yeni ürün API’leri değildir.
