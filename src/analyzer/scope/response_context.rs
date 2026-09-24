//! HTTP response facts for the current execution path. These are separate from
//! value taints: changing the response MIME type must not declassify secrets.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResponseContentType {
    #[default]
    Unknown,
    Html,
    Json,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum HeaderState {
    #[default]
    Mutable,
    Sent,
    MaybeSent,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResponseContext {
    pub content_type: ResponseContentType,
    headers: HeaderState,
}

impl ResponseContext {
    pub fn join(self, other: Self) -> Self {
        Self {
            content_type: if self.content_type == other.content_type {
                self.content_type
            } else {
                ResponseContentType::Unknown
            },
            headers: if self.headers == other.headers {
                self.headers
            } else {
                HeaderState::MaybeSent
            },
        }
    }

    pub fn set_content_type(&mut self, content_type: ResponseContentType) {
        match self.headers {
            HeaderState::Mutable => self.content_type = content_type,
            HeaderState::Sent => {}
            HeaderState::MaybeSent => {
                if self.content_type != content_type {
                    self.content_type = ResponseContentType::Unknown;
                }
            }
        }
    }

    pub fn emit(&mut self) {
        if self.headers == HeaderState::Mutable {
            self.headers = HeaderState::Sent;
        }
    }

    /// Calls with unknown response effects cannot preserve a MIME guarantee.
    /// As at function entry, we do not infer output hidden inside arbitrary
    /// callees; header mutability here tracks output observed on this path.
    pub fn invalidate(&mut self) {
        self.content_type = ResponseContentType::Unknown;
    }

    /// Buffering and control-flow edges without a response summary may also
    /// change whether headers have been committed.
    pub fn forget(&mut self) {
        self.invalidate();
        self.headers = HeaderState::MaybeSent;
    }
}

#[cfg(test)]
mod tests {
    use super::{ResponseContentType::*, ResponseContext};

    #[test]
    fn merges_keep_only_guaranteed_mime_and_header_mutability() {
        let mut json = ResponseContext::default();
        json.set_content_type(Json);
        assert_eq!(json.join(json).content_type, Json);
        assert_eq!(json.join(ResponseContext::default()).content_type, Unknown);
        let mut sent = json;
        sent.emit();
        let mut merged = json.join(sent);
        merged.set_content_type(Html);
        assert_eq!(merged.content_type, Unknown);
        sent.set_content_type(Html);
        assert_eq!(sent.content_type, Json);
    }
}
