# Lunixi Rust SDK

1.0.2 kanonik küçük birim string para sözleşmesini kullanır. [Geçiş rehberini](../MIGRATION-1.0.md) uygulayın.

Yeni kaynak yaması mağaza sitesi için somut bir sunum örneği ekler. **Varsayılan gömülü ödemedir**: yalnız resmi Lunixi bundle yüklenir ve mevcut form mount edilir; yöntem ve çoklu kart seçenekleri Lunixi panelinde kalır. İsteğe bağlı yönlendirme yalnız backend `paymentPageUrl` adresini, tam HTTPS `pay.lunixi.com/payment/` doğrulamasından sonra kullanır. Adresin token’ı gömülü token’dan farklı olabilir. Eksik adres gömülü ödemeyi engellemez; yönlendirme aynı deneme/anahtarla inceleme veya uzlaştırmada kalır. Tarayıcı token’ından adres üretmeyin. Formdan önce müşteri/sipariş sahipliğini ve yerel tarayıcı CSRF’yi doğrulayın; `Cache-Control: private, no-store` gönderin, mağazanın CSP nonce değerini koruyun; token, HTML ve yönlendirme adreslerini loglamayın. Genel yanıt verisi `paymentPageUrl` alanını korur; yeni tipli ürün eşitliği iddiası yoktur.

[English README](../README.md) · [Rust dokümantasyonu](https://docs.lunixi.com/tr/sdks/rust)

Tokio üzerinde çalışan, Rust 1.89+ (edition 2024) sunucu SDK'sıdır. Axum, Actix Web ve diğer Tokio uygulamalarında kullanılabilir.

## crates.io üzerinden kurulum

Yayımlanan [1.0.2 sürümünü](https://crates.io/crates/lunixi/1.0.2) Cargo ile ekleyin:

```sh
cargo add lunixi@1.0.2
```

Alternatif olarak uygulamanızın `Cargo.toml` dosyasını düzenleyin:

```toml
[dependencies]
lunixi = "2.0.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Kaynaktan kurulum için registry bağımlılığı yerine GitHub sürüm etiketi kullanılabilir:

```toml
[dependencies]
lunixi = { git = "https://github.com/Lunixi-Payment/lunixi-rust", tag = "v2.0.0" }
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
use lunixi::{Address, BasketItem, Buyer, IntentRequest};

let mut intent = IntentRequest::new("10050","TRY","ORD-1001","https://merchant.example/payment/return");
// Alıcı, fatura adresi ve sepet zorunludur (siparişinizden alın, yer tutucu kullanmayın);
// PHYSICAL kalem kargo adresini de ister. Alıcı IP'si intent'te isteğe bağlıdır:
// Lunixi formu IP'yi tarayıcıdan alır.
intent.buyer = Some(Buyer {
    name: "Ada".into(), surname: "Yılmaz".into(),
    identity_number: "11111111111".into(), email: "ada@example.com".into(),
    gsm_number: "+905000000000".into(), city: "Istanbul".into(),
    country: "Turkey".into(), zip_code: "34732".into(),
    ..Default::default()
});
intent.billing_address = Some(Address {
    contact_name: "Ada Yılmaz".into(), address: "Altunizade Mah. Örnek Sok. No: 1".into(),
    city: "Istanbul".into(), country: "Turkey".into(), zip_code: "34662".into(),
});
let mut line = BasketItem::new("line-1", "Danışmanlık hizmeti", "VIRTUAL", "10050");
line.category1 = Some("Services".into());
intent.basket_items.push(line);
let response = client.payments().create_intent(&intent,&kayitli_deneme_anahtari).await?;
```

Merchant başına bir istemci oluşturup klonlarını paylaşın; token önbelleği ve bağlantı havuzu ortaktır. Ed25519 PKCS8 PEM/base64 DER/32 bayt seed/doğrulanmış 64 bayt keypair kabul edilir. Açık anahtar panelde kaydedilir, özel anahtar sunucuda kalır. TEST/LIVE seçimini merchant anahtarı belirler. HTTPS zorunludur; yönlendirmeler, otomatik reqwest tekrarları ve ortam proxy'leri kapalıdır.

`amount` ve sepet `price` alanları kanonik alt birim string değeridir; `"10050"` = 100,50 TRY. Sepet toplamı intent tutarıyla aynı olmalıdır. Kaldırılmış kök alanlar `price`, `paidPrice`, `autoCapture` yerelde reddedilir. Taksit/vade farkını Lunixi formu yönetir; `enabled_installments` bu ödeme için formun sunacağı taksit sayılarını yalnızca daraltır (ör. en fazla 3 taksit için `Some(vec![1, 2, 3])`, yalnız tek çekim için `Some(vec![1])`). Form, panel ayarlarınızın, kartın ve platform üst sınırının da izin verdiği sayıları gösterir; `None` kısıt yok demektir. Sayılar 1–36 arası, tekrarsız olmalı ve 1'i içermelidir; aksi hâlde `create_intent` istek göndermeden `Error::Invalid` döner. Liste `enabledInstallments` olarak artan sırada gönderilir. İsteğe bağlı `BasketItem::product_category` (`productCategory` olarak gönderilir) Lunixi ürün kategorisidir; yasal taksit sınırı olan ödeme yöntemleri (ör. alışveriş kredisi) ve diğer platform özellikleri tarafından kullanılır. Değer için `lunixi::product_category` sabitlerini kullanın (`product_category::ALL` bu sürümün bildiği kodları listeler). `None` alanı göndermez ve mevcut davranışı korur; bilinmeyen kod API tarafından HTTP 400 ile reddedilir. Sözlük yalnız sona ekleme ile büyür; alan bu yüzden `Option<String>` olarak kalır. Her ödeme gövdesi sipariş bilgilerini taşır: `buyer` (`name`, `surname`, `identity_number` — T.C. kimlik veya pasaport numarası, `email` — ASCII, `gsm_number`, `city`, `country`, `zip_code`, `ip` — alıcının tarayıcı IP'si), `billing_address` (`contact_name`, `address`, `city`, `country`, `zip_code`) ve en az bir sepet kalemi (`id`, `name`, `category1`, `item_type`, `price`); `PHYSICAL` kalem `shipping_address` de ister. `create_intent`, `create_marketplace_intent` ve pazaryeri `create_checkout_intent` çağrılarında alıcı `ip` alanı verilmeyebilir (`CHECKOUT_INTENT_BUYER_OPTIONAL_FIELDS`). Bu çağrılar, `direct_method_payment`, `split`, pazaryeri kart tahsilatları, `store_card` (yalnız alıcı ve fatura adresi) ve cüzdan kartla yükleme, istek göndermeden eksik yolları (`buyer.email`, `basketItems.0.category1`, `shippingAddress`) adlandıran `Error::MissingOrderDetails` döndürür; `missing_order_details(&body, OrderDetailsScope::Payment)` aynı listeyi verir. Gateway eksik alanı HTTP 400 `payment.order.field_required`, geçersiz alanı `payment.order.field_invalid` ile yanıtlar (`order_field_error_code`, `order_field_reason`; listeler `ORDER_DETAILS_REQUIRED_FIELDS`, `BUYER_REQUIRED_FIELDS`, `ADDRESS_REQUIRED_FIELDS`, `BASKET_ITEM_REQUIRED_FIELDS`). Token yalnız ilgili siparişin yetkili müşterisine `Cache-Control: no-store` ile verilir ve mevcut Lunixi formu açılır. Token/PAN/CVV loglanmaz; varsayılan örnekler ham kart toplamaz.

## Kapsam ve güvenli tekrar

- `payments()`: `create_intent`, `get`, `list`, `methods` / `list_payment_methods`, `capture`, `refund`, `void`.
- `payments()` ödeme yöntemleri: `direct_method_payment`, `confirm_payment_action`, `resend_payment_action`.
- `payments()` senet: `get_promissory_notes`, `cancel_promissory_notes`, `download_promissory_note_document`.
- `payments()` Para Transferi: `list_bank_transfer_exceptions`, `accept_bank_transfer_exception`, `mark_bank_transfer_exception_refunded`.
- `payments()` parçalı ödeme: `split`, `get_split`, `list_splits`, `refund_split`.
- `payments()` kartlar: `bin_info`, `installment_options`, `card_taxonomy`, `store_card`, `list_stored_cards`, `get_stored_card`, `deactivate_stored_card`.
- `payments()` pazaryeri ve analiz: `create_marketplace_intent`, `failover_recovery_analytics`, `provider_credential_analytics`.
- `payment_links()`: `create`, `list`, `get`, `update`, `pause`, `resume`, `deactivate`, `action`, `send`, `qr`, `list_attempts`, `list_payments`, `create_image_upload`, `put_image_upload`, `confirm_image_upload`, `upload_image`.
- `reference()`: `list_provinces`, `list_districts`, `list_neighborhoods`.
- `physical_pos()`: `create_payment`, `create_reversal`, `cancel_payment`, `get_payment`, `list_payments`, `list_transactions`, `get_transaction`, `list_ambiguous_payments`, `get_sales_summary`, `get_tax_summary`, `list_settlements`, `start_settlement`, `list_settlement_runs`, `get_settlement_run`.
- `physical_pos_management()`: genel görünüm ve katalog, mağazalar, kasalar, banka ve sağlayıcı satırları ile cihaz bağlantılarını taşıyan mantıksal terminaller, kimlik anahtarlarıyla cihazlar, gün sonu politikası (hesap, mağaza, cihaz, geçerli olan).
- `fraud()`: `evaluate`, `record_event`, `record_batch`, `decisions`, `decision`, `latest_for_payment`.
- `wallet()`: `ping` ve `programs()`, `end_users()`, `wallets()`, `deposits()`, `corporate()`, `fees()`, `transfers()`, `withdrawals()`, `operations()`, `topups()`, `payments()`, `qr()`, `bulk_payouts()` alt yüzeyleri.
- `bank_connect()`: `list_banks`, `list_connections`, `get_connection`, `list_accounts`, `get_account`, `request_sync`, `list_balances`, `list_transactions`, `get_transaction`; `transfers()` (`list_purposes`, `create`, `list`, `get`), `fx()` (`list_rates`, `create_quote`, `create_order`, `list_orders`, `get_order`).
- `sanction()`: `screen_name`, `screen_wallet`, `screen_account`, `screen_transaction`, `screen_public_procurement_ban`, `screen_adverse_media`, `evaluate_behavioral_alert`, `list_screening_history`, izleme, varlık istihbaratı, `list_blacklist_entries`.
- `insurance()`: ürünler, referans değerleri, teklifler, siparişler, prim ödemesi ve iadesi, poliçe dosyaları, mutabakat, potansiyel müşteriler.
- `subscriptions()`: `create`, `get`, `list`, `cancel`, `pause`, `resume`, `change_plan`, `audit_logs`; `plans()`, `customers()`, `cards()`, `invoices()`, `portal()`, `products()`, `features()`, `addons()`, `usage()`.
- `kyc()`: `create_session`, `get_session`, `list_sessions`, `set_jurisdiction`, `set_primary_document`, `upload_document`, `timeline`, `submit`, `manual_review`, `evidence_request`, `merchant_policy`, `update_merchant_policy`, `rotate_webhook_secret`, `webhook_outbox`, `retry_webhook_event`, `erasure_requests`.
- `identity()`: `analyze_sync`, `analyze`, `account_links`, `get_session`, `related_sessions`, `timeline`, `top_sources`, `list_sessions`.
- `marketplace()`: bölünmüş tahsilatlar (`create_checkout_intent`, `charge_marketplace_card`, `charge_marketplace_card2d`, `charge_marketplace_card3d`), alt bayiler, komisyonlar, hakediş ödemeleri, denetim kaydı.
- `send(Request)`: diğer belgelenmiş `/api/v1/` merchant uçları için ortak imzalı taşıma.
- `WebhookVerifier` ve isteğe bağlı `SqliteInbox`: ham v2 doğrulama ve kalıcı transaction içinde tekrar koruması.

Finansal mutasyonlar 16–128 karakterlik boşluksuz yazdırılabilir ASCII anahtar ister. Anahtar işlemden önce kaydedilir; tekrar aynı gövde ve anahtarla gider. Varsayılan tekrar sıfırdır; yalnız GET/anahtarlı istekler yapılandırılan sınırda tekrarlanır. Anahtarsız mutasyon kör tekrarlanmaz. Belirsiz sonucu yeni ödeme başlatmadan önce uzlaştırın.

HTTP başarısı, token, tarayıcı dönüşü veya `AUTHORIZED` tahsilat değildir. Merchant/sipariş/ödeme kimliği, beklenen tutar/para birimi ve nihai tahsilat/uzlaştırma durumunu doğrulamadan siparişi tamamlamayın. Parçalı ödeme belgelenmiş JSON şemasını alır; tamsayı parça toplamlarını, 1–5 parçayı, tam dengeyi ve parça bloklarını denetler: kart parçası yöntem bloğu taşımaz, kart dışı parça tam olarak kendi `mobileBilling`, `wallet` veya `promissoryNote` bloğunu taşır (`MEAL_CARD`, `OPEN_BANKING`, `SHOPPING_LOAN` ve `BANK_TRANSFER` parça olamaz). Parçalı ödeme gövdesi (`SPLIT_PAYMENT_FIELDS`) sipariş bilgilerini de taşır: `buyer`, `billingAddress`, `basketItems` ve `PHYSICAL` kalem için `shippingAddress`. Yönteme özgü diğer alanları gateway doğrular. `list_splits`, `LIST_SPLIT_PAYMENTS_FILTERS` içindeki filtreleri sorgu çiftleri olarak alır. Parçalı ödeme webhook olay adları `split_payment_events` içindedir; parça olaylarını da almak için uç noktanın olay türlerine `SPLIT_LEG_EVENTS_SUBSCRIPTION` ekleyin. Parçalı ödeme beklerken HTTP 400 içeren `Response` dönebilir; ödenmiş sayılmaz. Yanıt zarfı, `data`, `page_info` ve ikili `bytes` korunur. Hatalarda uzak sunucunun gövdesi/mesajı/URL ve secret yoktur; veri erişimcileri yine müşteri bilgisi içerebilir ve loglanmamalıdır.

## Ödeme yöntemleri

`list_payment_methods(None)` (`methods` ile aynı) hesabınıza açık yöntemleri `blockName`, `providerSelection`, `providerSelectionMode` değerleri ve gönderilecek `inputFields` alanlarıyla sağlayıcılarıyla döndürür. Sağlayıcı kodunu her zaman bu yanıttan okuyun; kodda sabitlemeyin. `Some(code)` tek yöntemi listeler; kodlar yerelde denetlenir ve `QR`, `QR_CODE` olarak gönderilir. Kodlar ve değer kümeleri sabittir: `lunixi::payment_method`, `payment_action_type`, `provider_selection`, `provider_selection_mode`.

`direct_method_payment` herhangi bir yöntemle ödemeyi sunucunuzdan başlatır. Gövdeyi `DirectMethodPaymentRequest` ile kurun ya da ham bir `serde_json::Value` verin; ikisi de herhangi bir istekten önce denetlenir (yalnız bilinen alanlar, yalnız seçilen yöntemin bloğu, kart alanları yalnız `CARD` ile, blok yapısı, senet planı, 1–100 kalemlik alışveriş kredisi sepeti, sipariş bilgileri). Önceden kalıcılaştırılmış idempotency anahtarı zorunludur.

```rust
use lunixi::{DirectMethodPaymentRequest, payment_method};
use serde_json::json;

let request = DirectMethodPaymentRequest::new("10000", "TRY", "ORD-1001")
    .with_method_fields(
        payment_method::MOBILE_BILLING,
        Some(&provider_code), // from list_payment_methods
        json!({ "gsmNumber": "+905321234567" }),
    )
    // Sipariş bilgileri yukarıdaki gibi; burada alıcı `ip` alanı zorunludur.
    .with_buyer(buyer)
    .with_billing_address(billing_address)
    .with_basket_items(basket_items);
let response = client.payments()
    .direct_method_payment(&request, &persisted_attempt_key).await?;
```

`data.action` varsa (HTTP 202) alıcının işlem yapması gerekir: `OTP` ve `INPUT` adımları `confirm_payment_action(payment_id, action_id, json!({ "otp": "123456" }))` ile tamamlanır (yalnız `otp`, `consentAccepted` ve `fields` gönderilir), `resend_payment_action` yeni kod gönderir, `REDIRECT` alıcıyı `action.redirect.url` adresine yönlendirir. Kartlar `with_card(card)` ve 3D Secure için `with_force_3d(true)` kullanır.

### Para Transferi

`BANK_TRANSFER` sağlayıcı kodu almaz; alıcının bankasını `{ "bankCode": … }` olarak alır (diğer bankalar için `BANK_TRANSFER_OTHER_BANKS_CODE`, `"9999"`). Yanıt, transfer talimatını taşıyan bir `DISPLAY` adımı içerir:

```rust
use lunixi::{BankTransferInstruction, DirectMethodPaymentRequest, payment_method};

let request = DirectMethodPaymentRequest::new("10000", "TRY", "ORD-1002")
    .with_method_fields(payment_method::BANK_TRANSFER, None, json!({ "bankCode": "0062" }))
    .with_buyer(buyer)
    .with_billing_address(billing_address)
    .with_basket_items(basket_items);
let response = client.payments()
    .direct_method_payment(&request, &persisted_attempt_key).await?;
if let Some(instruction) = BankTransferInstruction::from_response(&response) {
    // Show iban, holder_name, amount_minor and payment_code to the buyer.
}
```

Para, bağlı banka hesabınızdan eşleştirilir; siparişi `payment.captured` olayında tamamlayın. Geç, eksik ve fazla gelen transferler istisna olur: `list_bank_transfer_exceptions(&BankTransferExceptionFilters::default().with_status("PENDING_DECISION"))`, ardından `accept_bank_transfer_exception(id)` ya da gönderene iadeyi kendiniz yaptıktan sonra `mark_bank_transfer_exception_refunded(id)`. Olaylar: `payment_bank_transfer_events`.

### Senetle Ödeme

`PROMISSORY_NOTE` bir plan ister: `with_promissory_note_schedule(vec![PromissoryNoteScheduleRow::new("5000", "2027-01-15"), …])` (1–24 satır, toplamı `amount` değerine eşit tutarlar, artan `YYYY-MM-DD` tarihler) ya da `with_promissory_note_count(n)` (1–24). Ödemeden sonra `get_promissory_notes(payment_id)` senetleri listeler, `cancel_promissory_notes(payment_id, None, None)` açık senetlerin tümünü iptal eder (bir kısmı için `Some(&["SN-…"])`) ve `download_promissory_note_document(payment_id, Some("tr-TR"))` yazdırılabilir PDF'i `Response::bytes` içinde döndürür. Borçlu adresleri `client.reference()` kodlarını kullanır: `list_provinces`, `list_districts("06")`, `list_neighborhoods(district_code)`. Olaylar: `promissory_note_events`.

## Kartlar, taksitler ve pazaryeri checkout'u

`bin_info("41111111")` 6–19 haneli bir önek için kart, banka ve taksit uygunluğunu döndürür. `installment_options(&InstallmentOptionsRequest::new("10000", "TRY").with_bin_or_pan(bin).with_installment(3))` taksit tablosunu döndürür; BIN ve 1'den büyük taksit verildiğinde `data`, `direct_method_payment` çağrısının `installmentQuoteToken` olarak gönderdiği `quoteToken` değerini taşır. `with_enabled_installments` intent ile aynı kuralı izler (1–36, tekrarsız, 1'i içerir). `card_taxonomy()` markaları, aileleri ve bankaları listeler.

`store_card(&StoreCardRequest::new(card, card_user_key, callback_url).with_buyer(buyer).with_billing_address(address), key)` kart sahibiniz için bir kart saklar (alıcı ve fatura adresi zorunludur); `list_stored_cards(card_user_key)`, `get_stored_card(token)` ve `deactivate_stored_card(token, key)` saklı kartları yönetir. Kart numarası ve güvenlik kodu debug çıktısında veya SDK hatalarında hiçbir zaman görünmez; isteği siz de loglamayın.

`create_marketplace_intent` bir `IntentRequest` (sipariş bilgileriyle) ve satıcılarını alır:

```rust
use lunixi::{MarketplaceIntentRequest, MarketplaceSeller};

let request = MarketplaceIntentRequest::new(intent, vec![
    MarketplaceSeller::new("dealer-1", "6000"),
    MarketplaceSeller::new("dealer-2", "4000"),
]);
let response = client.payments()
    .create_marketplace_intent(&request, &persisted_attempt_key).await?;
```

Satıcıların `amountMinor` değerlerinin toplamı `amount` değerine eşit olmalıdır; başka bir toplam, istek gönderilmeden yerelde hata verir.

## Ödeme linkleri

`client.payment_links()` ödeme linklerini oluşturur ve yönetir. Okumalar dahil her çağrı imzalıdır; ortamı imzalayan anahtar belirler.

```rust
use lunixi::{CreatePaymentLinkRequest, PaymentLinkSendRequest, payment_link_usage};

let request = CreatePaymentLinkRequest::fixed(
    payment_link_usage::SINGLE_USE, "150000", "TRY", "Invoice 42",
)
.with_recipient(json!({ "email": "buyer@example.com" }));
let link = client.payment_links().create(&request, &persisted_link_key).await?;
client.payment_links()
    .send(&link_id, &PaymentLinkSendRequest::new("EMAIL"), &persisted_send_key)
    .await?;
```

`create` ham bir `serde_json::Value` da kabul eder; ikisi de göndermeden önce `PAYMENT_LINK_SPEC_FIELDS` ve değer kümeleriyle denetlenir. `update(link_id, expected_row_version, &changes, key)` yalnız verilen anahtarları değiştirir (`null` bir alanı temizler; `usage`, `amountMode` ve `currency` sabittir). `pause`, `resume` ve `deactivate`, `PaymentLinkActionOptions` alır. `list`, `list_attempts` ve `list_payments`, `page_info` değerini korur (`hasMore`, `nextCursor`). `qr(link_id, Some("png"), Some(512))` görseli `Response::bytes` içinde döndürür.

Link görseli üç adımda ya da tek `upload_image(file_name, content_type, bytes)` çağrısıyla yüklenir: `create_image_upload` yükleme hedefini döndürür, `put_image_upload(&target, bytes)` dosyayı yalnız hedefin `uploadHeaders` başlıklarıyla doğrudan depolama sunucusuna gönderir (erişim token'ı veya istek imzası asla gönderilmez; yeniden denenmez) ve `confirm_image_upload(asset_id)` görseli yayımlar. Dönen `assetId` değerini bir linkin veya kalemin `imageAssetId` alanı olarak kullanın. En fazla 2 MiB PNG, JPEG ve WebP kabul edilir.

## Fiziki POS

`client.physical_pos()` mağaza içi cihazlarda kart ödemelerini başlatır ve izler; `client.physical_pos_management()` mağazaları, kasaları, mantıksal terminalleri, cihazları ve gün sonu politikasını yönetir. İstekler ve filtreler, göndermeden önce `PHYSICAL_POS_REQUEST_FIELDS` ve `PHYSICAL_POS_FILTERS` ile denetlenen JSON nesneleridir: bilinmeyen alanlar, yanlış tipler, eksik zorunlu alanlar, 1–2147483647 dışında `rowVersion` ve 1–100 dışında `limit` `Error::Invalid` olur. Para kanonik küçük birim metindir (`"4500"`); adetler ve miktarlar JSON sayısıdır.

```rust
use lunixi::{Error, PhysicalPos};
use serde_json::json;

let key = PhysicalPos::new_idempotency_key(); // store it with the order first
let sale = json!({
    "operation": "SALE",
    "target": { "deviceId": device_id },
    "amountMinor": "4500",
    "currency": "TRY",
    "merchantReference": order_id,
    "allowDuplicateReference": false,
});
match client.physical_pos().create_payment(&sale, &key).await {
    Ok(_written) => { /* 202 accepted (not paid) or 200 replayed: follow the webhooks */ }
    Err(Error::PhysicalPosOutcomeUnknown { .. }) => { /* the payment may exist: retry later with the SAME key */ }
    Err(other) => return Err(other.into()),
}
```

Ödeme yazımı "ödendi" değil, her zaman "kabul edildi" demektir: sonucu `physical_pos.payment.*` webhook'larıyla veya `get_payment` ile izleyin. `create_payment` ve `create_reversal` sakladığınız `Idempotency-Key` değerini ister (boşluksuz 8–200 yazdırılabilir ASCII karakter, verildiği gibi gönderilir); `cancel_payment`, `start_settlement` ve yönetim oluşturma çağrıları `Option<&str>` alır ve `None` için yeni bir UUID kullanır. Her yazım `data`, `replayed`, `status_code` (yeni için 202 veya 201, tekrar için 200) ve gönderilen `idempotency_key` ile bir `PhysicalPosWriteResult` döndürür. Ödeme yazımları kendi denemelerini (1 + `max_retries`) aynı anahtar ve gövdeyle çalıştırır; HTTP yanıtı gelmemesi, ödeme içermeyen 2xx ya da `PHYSICAL_POS_NOT_STARTED_CODES` dışında bir 5xx, sonraki bir deneme 4xx yanıt verse bile `Error::PhysicalPosOutcomeUnknown` olur. Aynı ödeme için yeni anahtar kartı iki kez çekebilir.

Kart verisi hiçbir zaman gönderilmez: kart cihazda okunur. Gövdenin veya filtrenin herhangi bir yerinde kart verisi alan adı (`pan`, `cardNumber`, `cvv`, `expiry` …) ya da `merchantReference`, ürün adı, vergi grubu veya `merchantReference` / `q` filtrelerinde kart numarası varsa, hiçbir şey gönderilmeden alan yoluyla `Error::PhysicalPosCardDataRejected` döner. Gateway yanıtları `Error::PhysicalPosCardDataRejected`, `Error::PhysicalPosVoidAfterSettlement` (bunun yerine iade yapın) ve `Error::PhysicalPosRateLimit` (`retry_after` ile) hatalarına eşlenir. Listeler `PhysicalPosPage` döndürür (`items`, `has_more`, `next_cursor`, `includeTotal: true` ile `total_count`); `next_cursor` değerini `cursor` filtresi olarak verin. Yol kimlikleri percent-encode edilir.

`PhysicalPosWebhookEvent::parse(&verifier, raw_body, &headers)` önce imzayı doğrular, sonra olayı okur: `category` (`payment`, `fiscal_document`, `settlement`, `other`), `history_seq`, `original` (ters kaydın asıl ödemesi), `cashier` (yalnız cihaz kasiyer oturumunda) ve `is_device_sale`; tutarlar doğrulamadan sonra denetlenir. Gün sonu olayları `historySeq` taşımaz; her olayı `id` ile tekilleştirin. Olay adları `physical_pos_events` ve `PHYSICAL_POS_EVENT_TYPES` içindedir.

## Cüzdan, banka hesapları, yaptırım ve sigorta

Bu istemciler istek gövdelerini ve filtreleri JSON nesnesi olarak alır (`serde_json::Value`, filtre yoksa `Value::Null`) ve hiçbir şey gönderilmeden önce denetler: bilinmeyen alan veya filtre, eksik zorunlu alan, kümesi dışında bir değer ya da hatalı tutar `Error::Invalid` olur ve süreçten hiçbir şey çıkmaz. Yanıtlar zarfın `data` alanıdır; listeler `CursorPage` döndürür (`items`, `has_more`, `next_cursor`, `total_count`, `raw`): `next_cursor` değerini değiştirmeden `cursor` filtresi olarak geri gönderin. Okumalar dahil her çağrı imzalıdır. Para hiçbir zaman JSON sayısı değil, kanonik küçük birim metindir (`"12550"` = 125,50 TRY).

**Cüzdan** (`client.wallet()`). Her rota `WALLET_ROUTES` içinde bir kez tanımlanır (metot, yol, `Idempotency-Key` kuralı, gövde ve sorgu alanları; `wallet_route(name)` birini bulur). Anahtar isteyen rotalar `idempotency_key: &str` alır (1–128 yazdırılabilir ASCII karakter, kırpılır); diğerleri almaz. Para iki adımda hareket eder: `initiate_*` `{operationId, integrityHash, nonce, otpRequired, …}` döndürür, `complete_*` işlemi kesinleştirir; complete çağrıları `operationId` değerini gövdeye kendileri ekler. Kartla yükleme (`topups().start_card`) `buyer` ve `billingAddress` ister (`shippingAddress` isteğe bağlı); eksik yolları göndermeden `Error::MissingOrderDetails` ile adlandırır.

```rust
use serde_json::json;

let wallet = client.wallet();
let started = wallet.transfers().initiate_w2w(&json!({
    "endUserId": end_user_id,
    "sourceWalletAccountId": account_id,
    "amount": "12550",
    "currency": "TRY",
    "destinationWalletNo": "1000000001",
}), &stored_key).await?;
let done = wallet.transfers().complete_w2w(started["operationId"].as_str().unwrap_or_default(), &json!({
    "endUserId": end_user_id,
    "integrityHash": started["integrityHash"],
    "nonce": started["nonce"],
}), &stored_complete_key).await?;
```

`qr().image(qr_id, format, scale)` bir `WalletQrImage` (SVG veya PNG baytları) döndürür. Değer kümeleri: `wallet_account_type`, `wallet_end_user_status`, `wallet_kyc_level`, `wallet_otp_channel`, `wallet_qr_type`, `wallet_qr_image_format`, `wallet_bulk_target_type`, `wallet_corporate_account_status`.

**Banka hesapları** (`client.bank_connect()`). Okuma filtreleri `BANK_CONNECT_FILTERS` kurallarını izler: 1–100 arası `limit`, en fazla 100 hesap kimliği, tek değer veya dizi olarak liste filtreleri (virgülle ayrılarak gönderilir), `minAmountMinor` ≤ `maxAmountMinor` koşuluyla küçük birim tutar filtreleri, yalnız `bookedFrom` ile birlikte `bookedTo` ve aralarında en fazla 92 gün. `transfers()` ve `fx()`, `BANK_TRANSFER_ROUTES` kurallarını izler; `bank-connect:transfer` kapsamını ve IP izin listesi olan bir anahtarı ister. `transfers().create(&body, key)` ve `fx().create_order(&body, key)` verdiğiniz anahtarı denetler (8–200 yazdırılabilir ASCII karakter, verildiği gibi gönderilir) ve `None` için yeni bir UUID kullanır; `Config::max_retries` ile istek ağ hatası veya 5xx sonrasında aynı anahtar ve gövdeyle yeniden denenir. Süreçler arasında güvenle yeniden denemek için kendi anahtarınızı saklayın (`BankConnectTransfers::new_idempotency_key()`); kullanılan anahtar `transfer.idempotencyKey` içinde de bulunur. `fx().create_quote`, `sourceAmountMinor` / `targetAmountMinor` alanlarından tam olarak birini ve iki farklı hesap alır.

**Yaptırım** (`client.sanction()`). Gövdeler `SANCTION_ROUTES`, `SANCTION_NESTED_FIELDS` ve `SANCTION_VALUE_SETS` kurallarını izler. Tarama karar değil eşleşme döndürür: `is_clear_screening(&result)` yalnız hiçbir eşleşme veya isabet, yaptırım / eşleşme / vaka işareti ya da engelleyici karar yoksa ve yanıt kapsam bildirdiğinde `coverageStatus` değeri `COMPLETE` ise true olur (`PARTIAL`, `DEGRADED` ve `null` hiçbir zaman temiz sayılmaz). Çalıştırılamayan tarama boş sonuç değil, `Error::Api` olur (HTTP 503 ya da `status` değeri `success` olmayan 2xx).

**Sigorta** (`client.insurance()`). `create_order`, `confirm_payment`, `issue_policy`, `cancel_order`, `refund_premium` ve `create_lead` bir `Idempotency-Key` ister (8–200 yazdırılabilir ASCII karakter); `create_quote` ve `bind_payment` isteğe bağlı anahtar alır. Teklifin sepeti ya `paymentIntentId` ya da `context.items` olur, ikisi birden olmaz. Bir sigorta türünün kendi nesnesine ait alan kuralları gateway'e bırakılır (bütün alan hatalarıyla 422). `get_policy_document`, `get_policy_receipt` ve `get_policy_certificate` bir `InsuranceDocument` döndürür (PDF baytları; hatalar JSON kalır); `export_reconciliation`, CSV'yi ve `truncated` / `next_cursor` / `aborted` son bilgisini taşıyan bir `InsuranceReconciliationExport` döndürür. `Insurance::consent_from_quote(&option, accepted_at)` siparişin `consent` değerini, `Insurance::premium_basket_item(&order, name, category1, category2)` bir `MERCHANT` siparişinin prim `BasketItem` değerini kurar.

**Webhook'lar.** `WebhookVerifier::verify` sonrasında veriyi `BankAccountTransactionReceived::from_event` (`bank_account.transaction.received`), `BankTransferWebhookData::from_event` (Bank Connect `bank_transfer.*`; ödemenin `payment.bank_transfer.*` olayları değil), `InsuranceWebhookData::from_event` (`insurance_events::ORDER_EVENTS`) ve `InsuranceLeadWebhookData::from_event` (`insurance.lead.submitted`) ile okuyun. Olay adları: `bank_connect_events`, `bank_transfer_events`, `insurance_events`, `sanction_events`; `*_EVENT_TYPES` listeleri ve abonelik kalıplarıyla.

## Abonelik, KYC, kimlik ve pazaryeri

Bu istemciler her rotayı bir tabloda bir kez tanımlar (`SUBSCRIPTION_ROUTES`, `KYC_ROUTES`, `IDENTITY_ROUTES`, `MARKETPLACE_ROUTES`: metot, yol, `Idempotency-Key` kuralı — `route.idempotency_key_rule()` değeri `"required"`, `"optional"` veya `"none"` olur —, gövde ve sorgu alanları) ve her çağrıyı hiçbir şey gönderilmeden önce bu tabloyla denetler. Gövdeler ve filtreler JSON nesneleridir (yoksa `Value::Null`); bilinmeyen alan veya filtre, eksik zorunlu alan, kümesi dışında bir değer ya da anahtar almayan bir rotada anahtar `Error::Invalid` olur ve hata hiçbir zaman değer taşımaz (kart verisi ve belge görselleri dahil). Yanıtlar zarfın `data` alanıdır; listeler `CursorPage` döndürür (`next_cursor` değerini `cursor` olarak, `page_size` de bildiren abonelik listelerinde ise `pageToken` olarak geri gönderin). Her çağrı imzalıdır.

**Abonelik** (`client.subscriptions()`). `create` bir `Idempotency-Key` ister (argüman veya isteğin kendi `idempotencyKey` alanı; ikisi birden verilirse aynı olmalıdır); `change_plan` gibi başlıkta ve gövdede gönderilir. Plan, müşteri, kart ve tahsilat yeniden denemesi yazımları isteğe bağlı anahtar alır (boşluksuz 1–200 yazdırılabilir ASCII karakter). Yenilemeler alıcıyı müşteri kaydından aldığı için `customers().upsert`; `externalId`, `name`, `surname`, `email`, `gsmNumber`, `identityNumber`, `billingAddress`, `billingCity`, `billingCountry` ve `billingZipCode` ister (`billingContactName` ve `shipping*` alanları isteğe bağlıdır; `phone`, `locale` ve `metadata` kabul edilmez); gateway eksik veya geçersiz alanı `subscription_customer_error_code` ile yanıtlar. Abonelik ve pazaryeri rotalarında gövdenin, filtrelerin ve yanıtın her para alanı kanonik küçük birim metin olmalıdır (`"9900"`; ücretsiz plan için `"0"`); yanıtta JSON sayısı `Error::Response` olur. `plans().create`, `price` değerini, para birimini ve `billing` kuralını denetler (`SUBSCRIPTION_BILLING_RULE_FIELDS`, büyük harfe çevrilen `frequency`, varsayılanı 1 olan `interval`). Kartlar checkout SDK'sının `paymentMethodToken` değeriyle eklenir; kart numarası alanı reddedilir. Portal linki rotaları `/api/panel/v1/subscriptions/portal` altındadır (istemcinin eriştiği tek panel yolları); `portal().create_sdk_session` zarfın kökünden okunan kanıt alanlarını döndürür, `subscription_portal_url(&link, None)` barındırılan adresi verir. Yardımcılar: `is_subscription_active`, `is_subscription_terminal`; değerler `subscription_*` modüllerinde ve `subscription_events` içindedir.

```rust
use serde_json::json;

let subscription = client.subscriptions().create(&json!({
    "customerId": customer_id,
    "planId": plan_id,
    "collectionMethod": "AUTO_CHARGE",
}), Some(&stored_key)).await?;
```

**KYC** (`client.kyc()`). `create_session` / `evidence_request`, `person` / `company` değerini `customerType` ile birlikte `customer` olarak, `metadata` değerini `evidenceContextJson` metni olarak gönderir; `sessionType` ve `ttlMinutes` (1–1440) değerlerini denetler ve isteğe bağlı anahtarı başlığa ve gövdeye koyar. `list_sessions` yalnız `limit` ve `cursor` alır (`pageSize` / `pageToken` aynı alanlar olarak okunur). Sonucun belirleyicisi `kyc.*` webhook'udur; `kyc_assurance_meets(achieved, required)` ve `is_kyc_session_terminal(status)` sonucu okur.

**Kimlik** (`client.identity()`). Karar değil, cihaz ve hesap ilişkilendirme sinyalleri: `is_identity_device_shared(&links, 2)` ve `identity_link_reasons(&links)` bir `account_links` yanıtını okur.

**Pazaryeri** (`client.marketplace()`). Bölünmüş tahsilatlar satıcı parçalarını `{subDealerId, amountMinor, scenario?}` öğelerinden oluşan bir JSON dizisi olarak (`scenario` bir JSON nesnesidir; `MarketplaceSeller` değerlerinin `serde_json::to_value(&sellers)` çıktısı aynı parçaları verir) ve `multi_seller_policy` değerini (varsayılan `ALL_OR_NOTHING`) alır; parçaların toplamı `amount` değerine eşit olmalıdır, aksi hâlde hiçbir şey gönderilmez. Buradaki her ödeme mutasyonu gibi önceden kalıcılaştırılmış 16–128 karakterlik bir `Idempotency-Key` isterler. `charge_marketplace_card2d` / `3d` ayrıca `MARKETPLACE_CHARGE_REQUIRED_FIELDS` alanlarını, en az bir sepet kalemini ve eksiksiz sipariş bilgilerini ister; `price` / `paidPrice` ve boolean olmayan `preauth` reddedilir. Kart verisi verildiği gibi gönderilir ve hiçbir hataya konmaz. `create_checkout_intent`, token veya form içermeyen yanıtı `Error::Api` hatasına çevirir. `marketplace_onboarding_url(&link, None)` bayi katılım adresini verir.

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
