//! A module for communicating with the [Bitbank API](https://github.com/bitbankinc/bitbank-api-docs/blob/master/README.md)

use std::{marker::PhantomData, time::SystemTime};

use crate::traits::*;
use generic_api_client::{http::*, websocket::*};
use header::HeaderValue;
use hmac::{Hmac, Mac};
use serde::{de::DeserializeOwned, Serialize};
use sha2::Sha256;

/// The type returned by [Client::request()].
pub type BitbankRequestResult<T> = Result<T, BitbankRequestError>;
pub type BitbankRequestError = RequestError<&'static str, BitbankHandleError>;

/// Options that can be set when creating handlers
#[derive(Default)]
pub enum BitbankOption {
    /// [Default] variant, does nothing
    #[default]
    Default,
    /// API key
    Key(String),
    /// API secret
    Secret(String),
    /// Base url for HTTP requests
    HttpUrl(BitbankHttpUrl),
    /// Whether [BitbankRequestHandler] should perform authentication
    HttpAuth(bool),
    /// [RequestConfig] used when sending requests.
    /// `url_prefix` will be overridden by [HttpUrl](Self::HttpUrl) unless `HttpUrl` is [BitbankHttpUrl::None].
    RequestConfig(RequestConfig),
    /// Base url for WebSocket connections.
    WebSocketUrl(BitbankWebSocketUrl),
    /// The channels to be subscribed by [WebSocketHandler].
    WebSocketChannels(Vec<String>),
    /// [WebSocketConfig] used for creating [WebSocketConnection]s.
    /// `url_prefix` will be overridden by [WebSocketUrl](Self::WebSocketUrl) unless `WebsocketUrl` is [BitbankWebSocketUrl::None].
    WebSocketConfig(WebSocketConfig),
}

/// A `struct` that represents a set of [BitbankOption]s.
#[derive(Clone, Debug)]
pub struct BitbankOptions {
    /// see [BitbankOption::Key]
    pub key: Option<String>,
    /// see [BitbankOption::Secret]
    pub secret: Option<String>,
    /// see [BitbankOption::HttpUrl]
    pub http_url: BitbankHttpUrl,
    /// see [BitbankOption::HttpAuth]
    pub http_auth: bool,
    /// see [BitbankOption::RequestConfig]
    pub request_config: RequestConfig,
    /// see [BitbankOption::WebSocketUrl]
    pub websocket_url: BitbankWebSocketUrl,
    /// see [BitbankOption::WebSocketChannels]
    pub websocket_channels: Vec<String>,
    /// see [BitbankOption::WebSocketConfig]
    pub websocket_config: WebSocketConfig,
}

/// A `enum` that represents the base url of the Bitbank HTTP API.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum BitbankHttpUrl {
    /// `https://api.bitbank.cc/v1`
    Private,
    /// `https://public.bitbank.cc/`
    Public,
    /// The url will not be modified by [BitbankRequestHandler]
    None,
}

/// A `enum` that represents the base url of the Bitbank WebSocket API.
#[derive(Debug, Eq, PartialEq, Copy, Clone)]
#[non_exhaustive]
pub enum BitbankWebSocketUrl {
    /// `wss://stream.bitbank.cc`
    Default,
    /// The url will not be modified by [BitbankWebSocketHandler]
    None,
}

/// <https://github.com/bitbankinc/bitbank-api-docs/blob/master/errors.md>
#[derive(Debug)]
pub enum BitbankHandleError {
    ApiError(serde_json::Value),
    RequestLimitExceeded(serde_json::Value), // Error code 10009 and HTTP status 429 https://github.com/bitbankinc/bitbank-api-docs/blob/master/rest-api.md#rate-limit
    ParseError,

    UrlNotFound(serde_json::Value),               // 10000
    SystemError(serde_json::Value),               // 10001 and 10003
    MalformedRequest(serde_json::Value),          // 10002
    TimeoutWaitingForResponse(serde_json::Value), // 10005
    SystemMaintenance(serde_json::Value),         // 10007
    ServerIsBusy(serde_json::Value),              // 10008
    RequestTooFrequent(serde_json::Value),        // 10009

    AuthenticationFailed(serde_json::Value),     // 20001
    InvalidAccessKey(serde_json::Value),         // 20002
    AccessKeyNotFound(serde_json::Value),        // 20003
    AccessNonceNotFound(serde_json::Value),      // 20004
    InvalidAccessSignature(serde_json::Value),   // 20005
    MfaFailed(serde_json::Value),                // 20011
    SmsVerificationFailed(serde_json::Value),    // 20014
    PleaseLogin(serde_json::Value),              // 20018
    MissingOtpCode(serde_json::Value),           // 20023
    MissingSmsCode(serde_json::Value),           // 20024
    MissingOtpAndSmsCode(serde_json::Value),     // 20025
    MfaTemporarilyLocked(serde_json::Value),     // 20026
    MissingAccessRequestTime(serde_json::Value), // 20033
    InvalidAccessRequestTime(serde_json::Value), // 20034, 20037
    NoRequestSentWithinAccessTimeWindow(serde_json::Value), // 20035
    AccessRequestTimeAndNonceNotFound(serde_json::Value), // 20036
    InvalidAccessTimeWindow(serde_json::Value),  // 20038
    InvalidAccessNonce(serde_json::Value),       // 20039

    MissingOrderQuantity(serde_json::Value),          // 30001
    MissingOrderId(serde_json::Value),                // 30006
    MissingOrderIds(serde_json::Value),               // 30007
    MissingAsset(serde_json::Value),                  // 30009, 30016
    MissingOrderPrice(serde_json::Value),             // 30012
    MissingSide(serde_json::Value),                   // 30013
    MissingOrderType(serde_json::Value),              // 30015
    MissingUuid(serde_json::Value),                   // 30019
    MissingWithdrawAmount(serde_json::Value),         // 30039
    MissingTriggerPrice(serde_json::Value),           // 30101
    MissingWithdrawalType(serde_json::Value),         // 30103
    MissingWithdrawalName(serde_json::Value),         // 30104
    MissingVasp(serde_json::Value),                   // 30105
    MissingBeneficiaryType(serde_json::Value),        // 30106
    MissingBeneficiaryLastName(serde_json::Value),    // 30107
    MissingBeneficiaryFirstName(serde_json::Value),   // 30108
    MissingBeneficiaryLastKana(serde_json::Value),    // 30109
    MissingBeneficiaryFirstKana(serde_json::Value),   // 30110
    MissingBeneficiaryCompanyName(serde_json::Value), // 30111
    MissingBeneficiaryCompanyKana(serde_json::Value), // 30112
    MissingBeneficiaryCompanyType(serde_json::Value), // 30113
    MissingBeneficiaryCompanyTypePosition(serde_json::Value), // 30114
    MissingUploadedDocuments(serde_json::Value),      // 30115
    MissingWithdrawalPurpose(serde_json::Value),      // 30116
    MissingBeneficiaryCountry(serde_json::Value),     // 30117
    MissingBeneficiaryZipCode(serde_json::Value),     // 30118
    MissingBeneficiaryPrefecture(serde_json::Value),  // 30119
    MissingBeneficiaryCity(serde_json::Value),        // 30120
    MissingBeneficiaryAddress(serde_json::Value),     // 30121
    MissingBeneficiaryBuilding(serde_json::Value),    // 30122
    MissingExtractionRequestCategory(serde_json::Value), // 30123

    InvalidOrderQuantity(serde_json::Value),    // 40001
    InvalidCount(serde_json::Value),            // 40006
    InvalidEndParam(serde_json::Value),         // 40007
    InvalidEndId(serde_json::Value),            // 40008
    InvalidFromId(serde_json::Value),           // 40009
    InvalidOrderId(serde_json::Value),          // 40013
    InvalidOrderIds(serde_json::Value),         // 40014
    TooManyOrdersSpecified(serde_json::Value),  // 40015
    InvalidAsset(serde_json::Value),            // 40017, 40025
    InvalidOrderPrice(serde_json::Value),       // 40020
    InvalidOrderSide(serde_json::Value),        // 40021
    InvalidTradingStartTime(serde_json::Value), // 40022
    InvalidOrderType(serde_json::Value),        // 40024
    InvalidUuid(serde_json::Value),             // 40028
    InvalidWithdrawAmount(serde_json::Value),   // 40048
    InvalidTriggerPrice(serde_json::Value),     // 40112
    InvalidPostOnly(serde_json::Value),         // 40113
    PostOnlyCannotBeSpecifiedWithSuchOrderType(serde_json::Value), // 40114
    InvalidWithdrawalType(serde_json::Value),   // 40116
    InvalidWithdrawalName(serde_json::Value),   // 40117
    InvalidVasp(serde_json::Value),             // 40118
    InvalidBeneficiaryType(serde_json::Value),  // 40119
    InvalidBeneficiaryLastName(serde_json::Value), // 40120
    InvalidBeneficiaryFirstName(serde_json::Value), // 40121
    InvalidBeneficiaryLastKana(serde_json::Value), // 40122
    InvalidBeneficiaryFirstKana(serde_json::Value), // 40123
    InvalidBeneficiaryCompanyName(serde_json::Value), // 40124
    InvalidBeneficiaryCompanyKana(serde_json::Value), // 40125
    InvalidBeneficiaryCompanyType(serde_json::Value), // 40126
    InvalidBeneficiaryCompanyTypePosition(serde_json::Value), // 40127
    InvalidOriginatorLabel(serde_json::Value),  // 40152
    InvalidOriginatorLastName(serde_json::Value), // 40153
    InvalidOriginatorFirstName(serde_json::Value), // 40154
    InvalidOriginatorCompanyName(serde_json::Value), // 40155
    InvalidOriginatorPrefecture(serde_json::Value), // 40156
    InvalidOriginatorCity(serde_json::Value),   // 40157
    InvalidOriginatorAddress(serde_json::Value), // 40158
    InvalidOriginatorBuilding(serde_json::Value), // 40159
    InvalidOriginatorSubstantialControllerName(serde_json::Value), // 40160
    InvalidBeneficiarySubstantialControllerName(serde_json::Value), // 40163

    AccountIsRestricted(serde_json::Value),  // 50003
    AccountIsProvisional(serde_json::Value), // 50004
    AccountIsBlocked(serde_json::Value),     // 50005, 50006
    IdentityVerificationIsNotFinished(serde_json::Value), // 50008
    OrderNotFound(serde_json::Value),        // 50009
    OrderCannotBeCanceled(serde_json::Value), // 50010
    ApiNotFound(serde_json::Value),          // 50011
    OrderHasAlreadyBeenCanceled(serde_json::Value), // 50026
    OrderHasAlreadyBeenExecuted(serde_json::Value), // 50027
    WithdrawalsToThisAddressRequireAdditionalEntries(serde_json::Value), // 50033
    VaspNotFound(serde_json::Value),         // 50034
    CompanyInformationIsNotRegistered(serde_json::Value), // 50035
    WithdrawalsTemporarilyRestricted(serde_json::Value), // 50037
    CannotWithdrawToChosenVaspService(serde_json::Value), // 50038
    OriginatorAlreadyRegistered(serde_json::Value), // 50043
    OriginatorNotFound(serde_json::Value),   // 50044
    DepositNotFound(serde_json::Value),      // 50045
    CannotEditBeneficiaryUnderReview(serde_json::Value), // 50046
    CannotEditDisabledBeneficiary(serde_json::Value), // 50047
    CannotWithdrawToBeneficiaryUnderReview(serde_json::Value), // 50048
    BeneficiaryRequiresAdditionalEntries(serde_json::Value), // 50049
    CannotWithdrawToChosenBeneficiary(serde_json::Value), // 50050
    CannotConfirmDepositWithOriginatorUnderReview(serde_json::Value), // 50051
    OriginatorRequiresAdditionalEntries(serde_json::Value), // 50052
    CannotEditOriginatorUnderReview(serde_json::Value), // 50053
    CannotWithdrawBecauseInformationRegistrationForUnreflectedDepositsHasNotBeCompleted(
        serde_json::Value,
    ), // 50054

    InsufficientAmount(serde_json::Value), // 60001
    MarketBuyOrderQuantityHasExceededTheUpperLimit(serde_json::Value), // 60002
    OrderQuantityHasExceededTheLimit(serde_json::Value), // 60003
    OrderQuantityHasExceededTheLowerThreshold(serde_json::Value), // 60004
    OrderPriceHasExceededTheUpperLimit(serde_json::Value), // 60005
    OrderPriceHasExceededTheLowerLimit(serde_json::Value), // 60006
    TooManySimultaneousOrders(serde_json::Value), // 60011
    TriggerPriceHasExceededTheUpperLimit(serde_json::Value), // 60016
    WithdrawalAmountHasExceededTheUpperLimit(serde_json::Value), // 60017

    SystemErrorStopUpdateRequest(serde_json::Value), // 70001, 70002, 70003, 70012
    OrderIsRestrictedDuringSuspensionOfTransactions(serde_json::Value), // 70004
    BuyOrderHasTemporarilyBeenRestricted(serde_json::Value), // 70005
    SellOrderHasTemporarilyBeenRestricted(serde_json::Value), // 70006
    MarketOrderHasTemporarilyBeenRestricted(serde_json::Value), // 70009, 70020
    MinimumOrderQuantityIsIncreasedTemporarily(serde_json::Value), // 70010
    SystemIsBusyStopUpdateRequest(serde_json::Value), // 70011
    OrderAndCancelHasTemporarilyBeenRestricted(serde_json::Value), // 70013
    WithdrawAndCancelRequestHasTemporarilyBeenRestricted(serde_json::Value), // 70014
    LendingAndCancelRequestHasTemporarilyBeenRestricted(serde_json::Value), // 70015
    LendingAndCancelRequestHasRestricted(serde_json::Value), // 70016
    OrdersOnPairHaveBeenSuspended(serde_json::Value), // 70017
    OrderAndCancelOnPairHaveBeenSuspended(serde_json::Value), // 70018
    OrderCancelRequestIsInProgress(serde_json::Value), // 70019
    LimitOrderPriceIsOverTheThreshold(serde_json::Value), // 70021
    StopLimitOrderHasTemporarilyBeenRestricted(serde_json::Value), // 70022
    StopOrderHasTemporarilyBeenRestricted(serde_json::Value), // 70023
}

/// A `struct` that implements [RequestHandler]
pub struct BitbankRequestHandler<'a, R: DeserializeOwned> {
    options: BitbankOptions,
    _phantom: PhantomData<&'a R>,
}

pub struct BitbankWebSocketHandler<H: FnMut(serde_json::Value) + Send> {
    message_handler: H,
    options: BitbankOptions,
}

// GET signs the path and query; POST signs the exact JSON bytes sent on the wire.
fn sign_request(request: &Request, secret: &str, timestamp: u128, window: u64) -> String {
    let mut hmac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    hmac.update(format!("{timestamp}{window}").as_bytes());
    if request.method() == Method::GET {
        hmac.update(request.url().path().as_bytes());
        if let Some(query) = request.url().query() {
            hmac.update(b"?");
            hmac.update(query.as_bytes());
        }
    } else {
        hmac.update(
            request
                .body()
                .and_then(|body| body.as_bytes())
                .unwrap_or_default(),
        );
    }
    hex::encode(hmac.finalize().into_bytes())
}

impl<'a, B, R> RequestHandler<B> for BitbankRequestHandler<'a, R>
where
    B: Serialize,
    R: DeserializeOwned,
{
    type Successful = R;
    type Unsuccessful = BitbankHandleError;
    type BuildError = &'static str;

    fn request_config(&self) -> RequestConfig {
        let mut config = self.options.request_config.clone();
        if self.options.http_url != BitbankHttpUrl::None {
            config.url_prefix = self.options.http_url.as_str().to_owned();
        }

        config
    }

    fn build_request(
        &self,
        mut builder: RequestBuilder,
        request_body: &Option<B>,
        _: u8,
    ) -> Result<Request, Self::BuildError> {
        if let Some(body) = request_body {
            let encoded = serde_json::to_string(&body)
                .or(Err("could not serialize body as application/json"))?;

            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(encoded);
        }

        // Authentication adds headers to the built request.
        let mut request = builder.build().or(Err("failed to build request"))?;

        if self.options.http_auth {
            // add authentication info to header
            // cf: https://github.com/bitbankinc/bitbank-api-docs/blob/master/rest-api.md
            // ACCESS-TIME-WINDOW method

            let access_request_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_err(|_| "system clock is before UNIX epoch")?
                .as_millis();
            let access_time_window = 5000;
            let secret = self
                .options
                .secret
                .as_deref()
                .ok_or("API secret not found")?;
            let signature = sign_request(&request, secret, access_request_time, access_time_window);

            let key =
                HeaderValue::from_str(self.options.key.as_deref().ok_or("API key not found")?)
                    .or(Err("invalid character in API key"))?;

            let headers = request.headers_mut();
            headers.insert("ACCESS-KEY", key);
            headers.insert(
                "ACCESS-REQUEST-TIME",
                HeaderValue::from_str(&access_request_time.to_string())
                    .map_err(|_| "invalid request time")?,
            );
            headers.insert("ACCESS-TIME-WINDOW", HeaderValue::from(access_time_window));
            headers.insert(
                "ACCESS-SIGNATURE",
                HeaderValue::from_str(&signature).unwrap(),
            );
        }

        Ok(request)
    }

    fn handle_response(
        &self,
        status: StatusCode,
        _: HeaderMap,
        response_body: Bytes,
    ) -> Result<Self::Successful, Self::Unsuccessful> {
        // API errors have a different shape from the caller's successful response type.
        let res_val: serde_json::Value =
            serde_json::from_slice(&response_body).map_err(|_| BitbankHandleError::ParseError)?;
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(BitbankHandleError::RequestLimitExceeded(res_val));
        }
        if !status.is_success() || res_val["success"].as_i64() == Some(0) {
            let Some(error_code) = res_val["data"]["code"].as_u64() else {
                return Err(BitbankHandleError::ApiError(res_val));
            };
            let error = match error_code {
                10000 => BitbankHandleError::UrlNotFound(res_val),
                10001 | 10003 => BitbankHandleError::SystemError(res_val),
                10002 => BitbankHandleError::MalformedRequest(res_val),
                10005 => BitbankHandleError::TimeoutWaitingForResponse(res_val),
                10007 => BitbankHandleError::SystemMaintenance(res_val),
                10008 => BitbankHandleError::ServerIsBusy(res_val),
                10009 => BitbankHandleError::RequestLimitExceeded(res_val),
                20001 => BitbankHandleError::AuthenticationFailed(res_val),
                20002 => BitbankHandleError::InvalidAccessKey(res_val),
                20003 => BitbankHandleError::AccessKeyNotFound(res_val),
                20004 => BitbankHandleError::AccessNonceNotFound(res_val),
                20005 => BitbankHandleError::InvalidAccessSignature(res_val),
                20011 => BitbankHandleError::MfaFailed(res_val),
                20014 => BitbankHandleError::SmsVerificationFailed(res_val),
                20018 => BitbankHandleError::PleaseLogin(res_val),
                20023 => BitbankHandleError::MissingOtpCode(res_val),
                20024 => BitbankHandleError::MissingSmsCode(res_val),
                20025 => BitbankHandleError::MissingOtpAndSmsCode(res_val),
                20026 => BitbankHandleError::MfaTemporarilyLocked(res_val),
                20033 => BitbankHandleError::MissingAccessRequestTime(res_val),
                20034 | 20037 => BitbankHandleError::InvalidAccessRequestTime(res_val),
                20035 => BitbankHandleError::NoRequestSentWithinAccessTimeWindow(res_val),
                20036 => BitbankHandleError::AccessRequestTimeAndNonceNotFound(res_val),
                20038 => BitbankHandleError::InvalidAccessTimeWindow(res_val),
                20039 => BitbankHandleError::InvalidAccessNonce(res_val),
                30001 => BitbankHandleError::MissingOrderQuantity(res_val),
                30006 => BitbankHandleError::MissingOrderId(res_val),
                30007 => BitbankHandleError::MissingOrderIds(res_val),
                30009 | 30016 => BitbankHandleError::MissingAsset(res_val),
                30012 => BitbankHandleError::MissingOrderPrice(res_val),
                30013 => BitbankHandleError::MissingSide(res_val),
                30015 => BitbankHandleError::MissingOrderType(res_val),
                30019 => BitbankHandleError::MissingUuid(res_val),
                30039 => BitbankHandleError::MissingWithdrawAmount(res_val),
                30101 => BitbankHandleError::MissingTriggerPrice(res_val),
                30103 => BitbankHandleError::MissingWithdrawalType(res_val),
                30104 => BitbankHandleError::MissingWithdrawalName(res_val),
                30105 => BitbankHandleError::MissingVasp(res_val),
                30106 => BitbankHandleError::MissingBeneficiaryType(res_val),
                30107 => BitbankHandleError::MissingBeneficiaryLastName(res_val),
                30108 => BitbankHandleError::MissingBeneficiaryFirstName(res_val),
                30109 => BitbankHandleError::MissingBeneficiaryLastKana(res_val),
                30110 => BitbankHandleError::MissingBeneficiaryFirstKana(res_val),
                30111 => BitbankHandleError::MissingBeneficiaryCompanyName(res_val),
                30112 => BitbankHandleError::MissingBeneficiaryCompanyKana(res_val),
                30113 => BitbankHandleError::MissingBeneficiaryCompanyType(res_val),
                30114 => BitbankHandleError::MissingBeneficiaryCompanyTypePosition(res_val),
                30115 => BitbankHandleError::MissingUploadedDocuments(res_val),
                30116 => BitbankHandleError::MissingWithdrawalPurpose(res_val),
                30117 => BitbankHandleError::MissingBeneficiaryCountry(res_val),
                30118 => BitbankHandleError::MissingBeneficiaryZipCode(res_val),
                30119 => BitbankHandleError::MissingBeneficiaryPrefecture(res_val),
                30120 => BitbankHandleError::MissingBeneficiaryCity(res_val),
                30121 => BitbankHandleError::MissingBeneficiaryAddress(res_val),
                30122 => BitbankHandleError::MissingBeneficiaryBuilding(res_val),
                30123 => BitbankHandleError::MissingExtractionRequestCategory(res_val),
                40001 => BitbankHandleError::InvalidOrderQuantity(res_val),
                40006 => BitbankHandleError::InvalidCount(res_val),
                40007 => BitbankHandleError::InvalidEndParam(res_val),
                40008 => BitbankHandleError::InvalidEndId(res_val),
                40009 => BitbankHandleError::InvalidFromId(res_val),
                40013 => BitbankHandleError::InvalidOrderId(res_val),
                40014 => BitbankHandleError::InvalidOrderIds(res_val),
                40015 => BitbankHandleError::TooManyOrdersSpecified(res_val),
                40017 | 40025 => BitbankHandleError::InvalidAsset(res_val),
                40020 => BitbankHandleError::InvalidOrderPrice(res_val),
                40021 => BitbankHandleError::InvalidOrderSide(res_val),
                40022 => BitbankHandleError::InvalidTradingStartTime(res_val),
                40024 => BitbankHandleError::InvalidOrderType(res_val),
                40028 => BitbankHandleError::InvalidUuid(res_val),
                40048 => BitbankHandleError::InvalidWithdrawAmount(res_val),
                40112 => BitbankHandleError::InvalidTriggerPrice(res_val),
                40113 => BitbankHandleError::InvalidPostOnly(res_val),
                40114 => BitbankHandleError::PostOnlyCannotBeSpecifiedWithSuchOrderType(res_val),
                40116 => BitbankHandleError::InvalidWithdrawalType(res_val),
                40117 => BitbankHandleError::InvalidWithdrawalName(res_val),
                40118 => BitbankHandleError::InvalidVasp(res_val),
                40119 => BitbankHandleError::InvalidBeneficiaryType(res_val),
                40120 => BitbankHandleError::InvalidBeneficiaryLastName(res_val),
                40121 => BitbankHandleError::InvalidBeneficiaryFirstName(res_val),
                40122 => BitbankHandleError::InvalidBeneficiaryLastKana(res_val),
                40123 => BitbankHandleError::InvalidBeneficiaryFirstKana(res_val),
                40124 => BitbankHandleError::InvalidBeneficiaryCompanyName(res_val),
                40125 => BitbankHandleError::InvalidBeneficiaryCompanyKana(res_val),
                40126 => BitbankHandleError::InvalidBeneficiaryCompanyType(res_val),
                40127 => BitbankHandleError::InvalidBeneficiaryCompanyTypePosition(res_val),
                40152 => BitbankHandleError::InvalidOriginatorLabel(res_val),
                40153 => BitbankHandleError::InvalidOriginatorLastName(res_val),
                40154 => BitbankHandleError::InvalidOriginatorFirstName(res_val),
                40155 => BitbankHandleError::InvalidOriginatorCompanyName(res_val),
                40156 => BitbankHandleError::InvalidOriginatorPrefecture(res_val),
                40157 => BitbankHandleError::InvalidOriginatorCity(res_val),
                40158 => BitbankHandleError::InvalidOriginatorAddress(res_val),
                40159 => BitbankHandleError::InvalidOriginatorBuilding(res_val),
                40160 => BitbankHandleError::InvalidOriginatorSubstantialControllerName(res_val),
                40163 => BitbankHandleError::InvalidBeneficiarySubstantialControllerName(res_val),
                50003 => BitbankHandleError::AccountIsRestricted(res_val),
                50004 => BitbankHandleError::AccountIsProvisional(res_val),
                50005 | 50006 => BitbankHandleError::AccountIsBlocked(res_val),
                50008 => BitbankHandleError::IdentityVerificationIsNotFinished(res_val),
                50009 => BitbankHandleError::OrderNotFound(res_val),
                50010 => BitbankHandleError::OrderCannotBeCanceled(res_val),
                50011 => BitbankHandleError::ApiNotFound(res_val),
                50026 => BitbankHandleError::OrderHasAlreadyBeenCanceled(res_val),
                50027 => BitbankHandleError::OrderHasAlreadyBeenExecuted(res_val),
                50033 => BitbankHandleError::WithdrawalsToThisAddressRequireAdditionalEntries(res_val),
                50034 => BitbankHandleError::VaspNotFound(res_val),
                50035 => BitbankHandleError::CompanyInformationIsNotRegistered(res_val),
                50037 => BitbankHandleError::WithdrawalsTemporarilyRestricted(res_val),
                50038 => BitbankHandleError::CannotWithdrawToChosenVaspService(res_val),
                50043 => BitbankHandleError::OriginatorAlreadyRegistered(res_val),
                50044 => BitbankHandleError::OriginatorNotFound(res_val),
                50045 => BitbankHandleError::DepositNotFound(res_val),
                50046 => BitbankHandleError::CannotEditBeneficiaryUnderReview(res_val),
                50047 => BitbankHandleError::CannotEditDisabledBeneficiary(res_val),
                50048 => BitbankHandleError::CannotWithdrawToBeneficiaryUnderReview(res_val),
                50049 => BitbankHandleError::BeneficiaryRequiresAdditionalEntries(res_val),
                50050 => BitbankHandleError::CannotWithdrawToChosenBeneficiary(res_val),
                50051 => BitbankHandleError::CannotConfirmDepositWithOriginatorUnderReview(res_val),
                50052 => BitbankHandleError::OriginatorRequiresAdditionalEntries(res_val),
                50053 => BitbankHandleError::CannotEditOriginatorUnderReview(res_val),
                50054 => BitbankHandleError::CannotWithdrawBecauseInformationRegistrationForUnreflectedDepositsHasNotBeCompleted(res_val),
                60001 => BitbankHandleError::InsufficientAmount(res_val),
                60002 => BitbankHandleError::MarketBuyOrderQuantityHasExceededTheUpperLimit(res_val),
                60003 => BitbankHandleError::OrderQuantityHasExceededTheLimit(res_val),
                60004 => BitbankHandleError::OrderQuantityHasExceededTheLowerThreshold(res_val),
                60005 => BitbankHandleError::OrderPriceHasExceededTheUpperLimit(res_val),
                60006 => BitbankHandleError::OrderPriceHasExceededTheLowerLimit(res_val),
                60011 => BitbankHandleError::TooManySimultaneousOrders(res_val),
                60016 => BitbankHandleError::TriggerPriceHasExceededTheUpperLimit(res_val),
                60017 => BitbankHandleError::WithdrawalAmountHasExceededTheUpperLimit(res_val),
                70001 | 70002 | 70003 | 70012 => BitbankHandleError::SystemErrorStopUpdateRequest(res_val),
                70004 => BitbankHandleError::OrderIsRestrictedDuringSuspensionOfTransactions(res_val),
                70005 => BitbankHandleError::BuyOrderHasTemporarilyBeenRestricted(res_val),
                70006 => BitbankHandleError::SellOrderHasTemporarilyBeenRestricted(res_val),
                70009 | 70020 => BitbankHandleError::MarketOrderHasTemporarilyBeenRestricted(res_val),
                70010 => BitbankHandleError::MinimumOrderQuantityIsIncreasedTemporarily(res_val),
                70011 => BitbankHandleError::SystemIsBusyStopUpdateRequest(res_val),
                70013 => BitbankHandleError::OrderAndCancelHasTemporarilyBeenRestricted(res_val),
                70014 => BitbankHandleError::WithdrawAndCancelRequestHasTemporarilyBeenRestricted(res_val),
                70015 => BitbankHandleError::LendingAndCancelRequestHasTemporarilyBeenRestricted(res_val),
                70016 => BitbankHandleError::LendingAndCancelRequestHasRestricted(res_val),
                70017 => BitbankHandleError::OrdersOnPairHaveBeenSuspended(res_val),
                70018 => BitbankHandleError::OrderAndCancelOnPairHaveBeenSuspended(res_val),
                70019 => BitbankHandleError::OrderCancelRequestIsInProgress(res_val),
                70021 => BitbankHandleError::LimitOrderPriceIsOverTheThreshold(res_val),
                70022 => BitbankHandleError::StopLimitOrderHasTemporarilyBeenRestricted(res_val),
                70023 => BitbankHandleError::StopOrderHasTemporarilyBeenRestricted(res_val),
                _ => BitbankHandleError::ApiError(res_val), // Unknown error code
            };
            return Err(error);
        }
        serde_json::from_slice(&response_body).map_err(|_| BitbankHandleError::ParseError)
    }
}

impl<H: FnMut(serde_json::Value) + Send + 'static> WebSocketHandler for BitbankWebSocketHandler<H> {
    fn websocket_config(&self) -> WebSocketConfig {
        let mut config = self.options.websocket_config.clone();

        if self.options.websocket_url != BitbankWebSocketUrl::None {
            config.url_prefix = self.options.websocket_url.as_str().to_owned();
        }

        config
    }

    fn handle_start(&mut self) -> Vec<WebSocketMessage> {
        // Wait for the Engine.IO OPEN packet before starting Socket.IO.
        vec![]
    }

    fn handle_message(&mut self, message: WebSocketMessage) -> Vec<WebSocketMessage> {
        let WebSocketMessage::Text(message) = message else {
            // Tungstenite handles WebSocket ping/pong; Engine.IO heartbeats are text frames.
            return vec![];
        };
        if let Some(payload) = message.strip_prefix('0') {
            if serde_json::from_str::<serde_json::Value>(payload)
                .is_ok_and(|value| value.is_object())
            {
                return vec![WebSocketMessage::Text("40".to_owned())];
            }
        } else if let Some(payload) = message.strip_prefix('2') {
            return vec![WebSocketMessage::Text(format!("3{payload}"))];
        } else if let Some(payload) = message.strip_prefix("40") {
            // Socket.IO CONNECT may omit the JSON payload.
            if payload.is_empty()
                || serde_json::from_str::<serde_json::Value>(payload)
                    .is_ok_and(|value| value.is_object())
            {
                return self
                    .options
                    .websocket_channels
                    .iter()
                    .map(|channel| {
                        WebSocketMessage::Text(format!(
                            "42{}",
                            serde_json::json!(["join-room", channel])
                        ))
                    })
                    .collect();
            }
        } else if let Some(payload) = message.strip_prefix("42") {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(payload) {
                if value.is_array() {
                    (self.message_handler)(value);
                }
            }
        }
        vec![]
    }

    fn handle_close(&mut self, reconnect: bool) {
        log::debug!(
            "Bitbank WebSocket connection closed; reconnect: {}",
            reconnect
        );
    }
}

impl BitbankHttpUrl {
    /// The base URL that this variant represents.
    #[inline(always)]
    fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "https://public.bitbank.cc",
            Self::Private => "https://api.bitbank.cc/v1",
            Self::None => "",
        }
    }
}

impl BitbankWebSocketUrl {
    /// The base URL that this variant represents.
    #[inline(always)]
    fn as_str(&self) -> &'static str {
        match self {
            // Since bitbank's stream API is implemented with socket.io, it becomes complicated without using socket.io library.
            Self::Default => "wss://stream.bitbank.cc/socket.io/?EIO=4&transport=websocket",
            Self::None => "",
        }
    }
}

impl HandlerOptions for BitbankOptions {
    type OptionItem = BitbankOption;

    fn update(&mut self, option: Self::OptionItem) {
        match option {
            BitbankOption::Default => (),
            BitbankOption::Key(v) => self.key = Some(v),
            BitbankOption::Secret(v) => self.secret = Some(v),
            BitbankOption::HttpUrl(v) => self.http_url = v,
            BitbankOption::HttpAuth(v) => self.http_auth = v,
            BitbankOption::RequestConfig(v) => self.request_config = v,
            BitbankOption::WebSocketUrl(v) => self.websocket_url = v,
            BitbankOption::WebSocketChannels(v) => self.websocket_channels = v,
            BitbankOption::WebSocketConfig(v) => self.websocket_config = v,
        }
    }
}

impl Default for BitbankOptions {
    fn default() -> Self {
        Self {
            key: None,
            secret: None,
            http_url: BitbankHttpUrl::None,
            http_auth: false,
            request_config: RequestConfig::default(),
            websocket_url: BitbankWebSocketUrl::Default,
            websocket_channels: vec![],
            websocket_config: WebSocketConfig::default(),
        }
    }
}

impl<'a, R, B> HttpOption<'a, R, B> for BitbankOption
where
    R: DeserializeOwned + 'a,
    B: Serialize,
{
    type RequestHandler = BitbankRequestHandler<'a, R>;

    #[inline(always)]
    fn request_handler(options: Self::Options) -> Self::RequestHandler {
        BitbankRequestHandler::<'a, R> {
            options,
            _phantom: PhantomData,
        }
    }
}

impl<H: FnMut(serde_json::Value) + Send + 'static> WebSocketOption<H> for BitbankOption {
    type WebSocketHandler = BitbankWebSocketHandler<H>;

    #[inline(always)]
    fn websocket_handler(handler: H, options: Self::Options) -> Self::WebSocketHandler {
        BitbankWebSocketHandler {
            message_handler: handler,
            options,
        }
    }
}

impl HandlerOption for BitbankOption {
    type Options = BitbankOptions;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::{json, Value};

    fn handler<R: DeserializeOwned>() -> BitbankRequestHandler<'static, R> {
        BitbankRequestHandler {
            options: BitbankOptions::default(),
            _phantom: PhantomData,
        }
    }

    fn response<R: DeserializeOwned + 'static>(
        status: StatusCode,
        body: &str,
    ) -> Result<R, BitbankHandleError> {
        <BitbankRequestHandler<R> as RequestHandler<()>>::handle_response(
            &handler(),
            status,
            HeaderMap::new(),
            Bytes::copy_from_slice(body.as_bytes()),
        )
    }

    #[test]
    fn signatures_match_bitbank_official_vectors() {
        // https://github.com/bitbankinc/bitbank-api-docs/blob/master/rest-api.md#access-time-window
        let get = reqwest::Client::new()
            .get("https://api.bitbank.cc/v1/user/assets")
            .build()
            .unwrap();
        assert_eq!(
            sign_request(&get, "hoge", 1721121776490, 1000),
            "9ec5745960d05573c8fb047cdd9191bd0c6ede26f07700bb40ecf1a3920abae8"
        );
        let post = reqwest::Client::new().post("https://api.bitbank.cc/v1/user/spot/order")
            .body(r#"{"pair": "xrp_jpy", "price": "20", "amount": "1","side": "buy", "type": "limit"}"#)
            .build().unwrap();
        assert_eq!(
            sign_request(&post, "hoge", 1721121776490, 1000),
            "7868665738ae3f8a796224e0413c1351ddd7ec2af121db12815c0a5b74b8764c"
        );
    }

    #[test]
    fn signature_covers_query_and_empty_post_body() {
        let client = reqwest::Client::new();
        let get = client
            .get("https://api.bitbank.cc/v1/user/spot/order?pair=btc_jpy&order_id=42")
            .build()
            .unwrap();
        let post = client
            .post("https://api.bitbank.cc/v1/user/spot/order")
            .build()
            .unwrap();
        // Independently generated with Python's standard-library hmac.
        assert_eq!(
            sign_request(&get, "secret", 123, 5000),
            "ce9d9c8c302dce28f5bb6f386517bc8e8bf27e6d4d612da94bdfd275fe5ff0ab"
        );
        assert_eq!(
            sign_request(&post, "secret", 123, 5000),
            "f27ea703c22c3d7bed42d89cd691922c4707a4a56d32ffe8688b8e04d5519415"
        );
    }

    #[test]
    fn request_headers_sign_exact_serialized_body() {
        let mut handler = handler::<Value>();
        handler.options.http_auth = true;
        handler.options.key = Some("key".into());
        handler.options.secret = Some("secret".into());
        let body = json!({"pair":"btc_jpy", "amount":"0.001"});
        let request = handler
            .build_request(
                reqwest::Client::new().post("https://api.bitbank.cc/v1/user/spot/order"),
                &Some(body),
                0,
            )
            .unwrap();
        let headers = request.headers();
        let time = headers["ACCESS-REQUEST-TIME"]
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(headers["ACCESS-KEY"], "key");
        assert_eq!(headers["ACCESS-TIME-WINDOW"], "5000");
        assert_eq!(headers[header::CONTENT_TYPE], "application/json");
        assert_eq!(
            headers["ACCESS-SIGNATURE"],
            sign_request(&request, "secret", time, 5000)
        );
    }

    #[test]
    fn missing_or_invalid_credentials_are_build_errors() {
        let mut handler = handler::<Value>();
        handler.options.http_auth = true;
        let build = |handler: &BitbankRequestHandler<Value>| {
            handler.build_request(
                reqwest::Client::new().get("https://api.bitbank.cc/v1/user/assets"),
                &None::<()>,
                0,
            )
        };
        assert_eq!(build(&handler).unwrap_err(), "API secret not found");
        handler.options.secret = Some("secret".into());
        assert_eq!(build(&handler).unwrap_err(), "API key not found");
        handler.options.key = Some("invalid\nkey".into());
        assert_eq!(build(&handler).unwrap_err(), "invalid character in API key");
    }

    #[derive(Debug, Deserialize)]
    struct Success {
        success: u8,
        data: Price,
    }
    #[derive(Debug, Deserialize)]
    struct Price {
        last: String,
    }

    #[test]
    fn typed_success_and_api_errors_are_distinguished() {
        let success: Success =
            response(StatusCode::OK, r#"{"success":1,"data":{"last":"123"}}"#).unwrap();
        assert_eq!(success.success, 1);
        assert_eq!(success.data.last, "123");
        for status in [StatusCode::OK, StatusCode::BAD_REQUEST] {
            assert!(matches!(
                response::<Success>(status, r#"{"success":0,"data":{"code":30012}}"#),
                Err(BitbankHandleError::MissingOrderPrice(_))
            ));
        }
        for (status, body) in [
            (StatusCode::OK, r#"{"success":0,"data":{"code":10009}}"#),
            (
                StatusCode::TOO_MANY_REQUESTS,
                r#"{"message":"rate limited"}"#,
            ),
        ] {
            assert!(matches!(
                response::<Success>(status, body),
                Err(BitbankHandleError::RequestLimitExceeded(_))
            ));
        }
        assert!(matches!(
            response::<Success>(StatusCode::BAD_GATEWAY, "{}"),
            Err(BitbankHandleError::ApiError(_))
        ));
        assert!(matches!(
            response::<Success>(StatusCode::OK, r#"{"success":0,"data":{"code":99999}}"#),
            Err(BitbankHandleError::ApiError(_))
        ));
        assert!(matches!(
            response::<Success>(StatusCode::OK, "not JSON"),
            Err(BitbankHandleError::ParseError)
        ));
        assert!(matches!(
            response::<Success>(StatusCode::OK, r#"{"success":1,"data":{}}"#),
            Err(BitbankHandleError::ParseError)
        ));
    }

    #[test]
    fn websocket_handles_malformed_frames_and_heartbeats() {
        let mut handler = BitbankOption::websocket_handler(
            |_| panic!("unexpected callback"),
            BitbankOptions::default(),
        );
        for text in [
            "", "4", "4あ", "あ", "0{", "40{", "42{", "42null", "42{}", "1",
        ] {
            assert!(handler
                .handle_message(WebSocketMessage::Text(text.into()))
                .is_empty());
        }
        for message in [
            WebSocketMessage::Binary(vec![0]),
            WebSocketMessage::Ping(vec![1]),
            WebSocketMessage::Pong(vec![2]),
        ] {
            assert!(handler.handle_message(message).is_empty());
        }
        assert_eq!(
            handler.handle_message(WebSocketMessage::Text("2probe".into())),
            vec![WebSocketMessage::Text("3probe".into())]
        );
    }

    #[test]
    fn websocket_escapes_channels_and_delivers_events() {
        use std::sync::{Arc, Mutex};
        let received = Arc::new(Mutex::new(Vec::new()));
        let captured = received.clone();
        let options = BitbankOptions {
            websocket_channels: vec!["channel\"\\name".into()],
            ..Default::default()
        };
        let mut handler = BitbankOption::websocket_handler(
            move |value| captured.lock().unwrap().push(value),
            options,
        );
        let joins = handler.handle_message(WebSocketMessage::Text("40".into()));
        let WebSocketMessage::Text(join) = &joins[0] else {
            panic!("expected text")
        };
        assert_eq!(
            serde_json::from_str::<Value>(&join[2..]).unwrap(),
            json!(["join-room", "channel\"\\name"])
        );
        handler.handle_message(WebSocketMessage::Text(
            r#"42["message",{"price":"123"}]"#.into(),
        ));
        assert_eq!(
            *received.lock().unwrap(),
            vec![json!(["message", {"price":"123"}])]
        );
    }

    #[tokio::test]
    async fn websocket_resubscribes_after_real_reconnection() {
        use futures_util::{SinkExt, StreamExt};
        use std::time::Duration;
        use tokio::net::TcpListener;
        use tokio_tungstenite::{accept_async, tungstenite::Message};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (joined_tx, mut joined_rx) = tokio::sync::mpsc::unbounded_channel();
        let server = tokio::spawn(async move {
            let mut clients = tokio::task::JoinSet::new();
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let joined_tx = joined_tx.clone();
                clients.spawn(async move {
                    let mut ws = accept_async(stream).await.unwrap();
                    ws.send(Message::Text(
                        r#"0{"sid":"test","pingInterval":25000,"pingTimeout":20000}"#.into(),
                    ))
                    .await
                    .unwrap();
                    assert_eq!(
                        ws.next().await.unwrap().unwrap(),
                        Message::Text("40".into())
                    );
                    ws.send(Message::Text(r#"40{"sid":"test"}"#.into()))
                        .await
                        .unwrap();
                    assert_eq!(
                        ws.next().await.unwrap().unwrap(),
                        Message::Text(r#"42["join-room","transactions_btc_jpy"]"#.into())
                    );
                    ws.send(Message::Text("2".into())).await.unwrap();
                    assert_eq!(ws.next().await.unwrap().unwrap(), Message::Text("3".into()));
                    joined_tx.send(()).unwrap();
                    let _ = ws.next().await;
                });
            }
            while let Some(result) = clients.join_next().await {
                result.unwrap();
            }
        });
        let mut config = WebSocketConfig::default();
        config.connect_cooldown = Duration::from_millis(10);
        let options = BitbankOptions {
            websocket_url: BitbankWebSocketUrl::None,
            websocket_channels: vec!["transactions_btc_jpy".into()],
            websocket_config: config,
            ..Default::default()
        };
        let connection =
            WebSocketConnection::new(&url, BitbankOption::websocket_handler(|_| {}, options))
                .await
                .unwrap();
        tokio::time::timeout(Duration::from_secs(5), joined_rx.recv())
            .await
            .unwrap()
            .unwrap();
        connection.reconnect_state().request_reconnect();
        tokio::time::timeout(Duration::from_secs(5), joined_rx.recv())
            .await
            .unwrap()
            .unwrap();
        drop(connection);
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }
}
