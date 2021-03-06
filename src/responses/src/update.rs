/*!
Response types for a [update document request](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-update.html).
*/

use parsing::{HttpResponseHead, IsOk, MaybeOkResponse, ResponseBody, Unbuffered};
use common::DocumentResult;
use error::*;

/** Response for a [update document request](https://www.elastic.co/guide/en/elasticsearch/reference/current/docs-update.html). */
#[derive(Deserialize, Debug)]
pub struct UpdateResponse {
    #[serde(rename = "_index")] index: String,
    #[serde(rename = "_type")] ty: String,
    #[serde(rename = "_id")] id: String,
    #[serde(rename = "_version")] version: Option<u32>,
    #[serde(rename = "_routing")] routing: Option<String>,
    result: DocumentResult,
}

impl UpdateResponse {
    /** Whether or not the document was updated. */
    pub fn updated(&self) -> bool {
        match self.result {
            DocumentResult::Updated => true,
            _ => false,
        }
    }

    /** The index for the document. */
    pub fn index(&self) -> &str {
        &self.index
    }

    /** The type of the document. */
    pub fn ty(&self) -> &str {
        &self.ty
    }

    /** The id of the document. */
    pub fn id(&self) -> &str {
        &self.id
    }

    /** The version of the document. */
    pub fn version(&self) -> Option<u32> {
        self.version.clone()
    }
}

impl IsOk for UpdateResponse {
    fn is_ok<B: ResponseBody>(head: HttpResponseHead, body: Unbuffered<B>) -> Result<MaybeOkResponse<B>, ParseResponseError> {
        match head.status() {
            200...299 => Ok(MaybeOkResponse::ok(body)),
            _ => Ok(MaybeOkResponse::err(body)),
        }
    }
}
