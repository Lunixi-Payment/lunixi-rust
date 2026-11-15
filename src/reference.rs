use crate::{Client, Error, Method, Request, Response, Result};

const BASE: &str = "/api/v1/reference/tr-address";

/// Lunixi address dictionary for Türkiye (province → district → neighborhood).
/// The codes fill an `ADDRESS` input field, e.g. the debtor address of a
/// promissory-note payment: `{ provinceCode, districtCode, neighborhoodCode, line }`.
/// `data.items` is `[{ code, name }]`; an empty list means the data for that
/// level is not published yet.
pub struct Reference<'a> {
    pub(crate) client: &'a Client,
}
impl Client {
    /// Returns the reference data facade sharing the same transport.
    pub fn reference(&self) -> Reference<'_> {
        Reference { client: self }
    }
}
impl Reference<'_> {
    /// Provinces with their Lunixi code (`01`–`81`).
    pub async fn list_provinces(&self) -> Result<Response> {
        self.client
            .send(Request::new(Method::GET, format!("{BASE}/provinces")))
            .await
    }
    /// Districts of a province (`province_code` `01`–`81`).
    pub async fn list_districts(&self, province_code: &str) -> Result<Response> {
        let b = province_code.as_bytes();
        let valid = b.len() == 2
            && b.iter().all(u8::is_ascii_digit)
            && province_code
                .parse::<u8>()
                .is_ok_and(|n| (1..=81).contains(&n));
        if !valid {
            return Err(Error::Invalid("province code (01-81)"));
        }
        self.list(format!("{BASE}/districts"), "provinceCode", province_code)
            .await
    }
    /// Neighborhoods of a district (`district_code` from `list_districts`).
    pub async fn list_neighborhoods(&self, district_code: &str) -> Result<Response> {
        if !(1..=12).contains(&district_code.len())
            || !district_code.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(Error::Invalid("district code (digits from list_districts)"));
        }
        self.list(
            format!("{BASE}/neighborhoods"),
            "districtCode",
            district_code,
        )
        .await
    }
    async fn list(&self, path: String, key: &str, value: &str) -> Result<Response> {
        let mut request = Request::new(Method::GET, path);
        request.query.push((key.into(), value.into()));
        self.client.send(request).await
    }
}
