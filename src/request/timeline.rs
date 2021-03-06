pub use self::inner::{Content, Reach, Scope, Stream};
use super::err::Timeline as Error;
use super::query::Query;
pub(crate) use inner::UserData;

use lru::LruCache;
use warp::reject::Rejection;

mod err;
mod inner;

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Copy, Eq, Hash, PartialEq)]
pub struct Timeline(pub Stream, pub Reach, pub Content);

impl Timeline {
    pub fn empty() -> Self {
        Self(Stream::Unset, Reach::Local, Content::Notification)
    }

    pub(crate) fn is_public(&self) -> bool {
        if let Self(Stream::Public, _, _) = self {
            true
        } else {
            false
        }
    }

    pub(crate) fn tag(&self) -> Option<i64> {
        if let Self(Stream::Hashtag(id), _, _) = self {
            Some(*id)
        } else {
            None
        }
    }

    pub(crate) fn to_redis_raw_timeline(&self, hashtag: Option<&String>) -> Result<String> {
        use {Content::*, Error::*, Reach::*, Stream::*};

        Ok(match self {
            Timeline(Public, Federated, All) => "timeline:public".to_string(),
            Timeline(Public, Local, All) => "timeline:public:local".to_string(),
            Timeline(Public, Federated, Media) => "timeline:public:media".to_string(),
            Timeline(Public, Local, Media) => "timeline:public:local:media".to_string(),
            Timeline(Hashtag(_id), Federated, All) => {
                ["timeline:hashtag:", hashtag.ok_or(MissingHashtag)?].concat()
            }
            Timeline(Hashtag(_id), Local, All) => [
                "timeline:hashtag:",
                hashtag.ok_or(MissingHashtag)?,
                ":local",
            ]
            .concat(),
            Timeline(User(id), Federated, All) => ["timeline:", &id.to_string()].concat(),
            Timeline(User(id), Federated, Notification) => {
                ["timeline:", &id.to_string(), ":notification"].concat()
            }
            Timeline(List(id), Federated, All) => ["timeline:list:", &id.to_string()].concat(),
            Timeline(Direct(id), Federated, All) => ["timeline:direct:", &id.to_string()].concat(),
            Timeline(_one, _two, _three) => Err(Error::InvalidInput)?,
        })
    }

    pub fn from_redis_text(timeline: &str, cache: &mut LruCache<String, i64>) -> Result<Self> {
        use {Content::*, Error::*, Reach::*, Stream::*};
        let mut tag_id = |t: &str| cache.get(&t.to_string()).map_or(Err(BadTag), |id| Ok(*id));

        Ok(match &timeline.split(':').collect::<Vec<&str>>()[..] {
            ["public"] => Timeline(Public, Federated, All),
            ["public", "local"] => Timeline(Public, Local, All),
            ["public", "media"] => Timeline(Public, Federated, Media),
            ["public", "local", "media"] => Timeline(Public, Local, Media),
            ["hashtag", tag] => Timeline(Hashtag(tag_id(tag)?), Federated, All),
            ["hashtag", tag, "local"] => Timeline(Hashtag(tag_id(tag)?), Local, All),
            [id] => Timeline(User(id.parse()?), Federated, All),
            [id, "notification"] => Timeline(User(id.parse()?), Federated, Notification),
            ["list", id] => Timeline(List(id.parse()?), Federated, All),
            ["direct", id] => Timeline(Direct(id.parse()?), Federated, All),
            [..] => Err(InvalidInput)?, // Other endpoints don't exist
        })
    }

    pub(crate) fn from_query_and_user(
        q: &Query,
        user: &UserData,
    ) -> std::result::Result<Self, Rejection> {
        use {warp::reject::custom, Content::*, Reach::*, Scope::*, Stream::*};

        Ok(match q.stream.as_ref() {
            "public" => match q.media {
                true => Timeline(Public, Federated, Media),
                false => Timeline(Public, Federated, All),
            },
            "public:local" => match q.media {
                true => Timeline(Public, Local, Media),
                false => Timeline(Public, Local, All),
            },
            "public:media" => Timeline(Public, Federated, Media),
            "public:local:media" => Timeline(Public, Local, Media),

            "hashtag" => Timeline(Hashtag(0), Federated, All),
            "hashtag:local" => Timeline(Hashtag(0), Local, All),
            "user" => match user.scopes.contains(&Statuses) {
                true => Timeline(User(user.id), Federated, All),
                false => Err(custom("Error: Missing access token"))?,
            },
            "user:notification" => match user.scopes.contains(&Statuses) {
                true => Timeline(User(user.id), Federated, Notification),
                false => Err(custom("Error: Missing access token"))?,
            },
            "list" => match user.scopes.contains(&Lists) {
                true => Timeline(List(q.list), Federated, All),
                false => Err(warp::reject::custom("Error: Missing access token"))?,
            },
            "direct" => match user.scopes.contains(&Statuses) {
                true => Timeline(Direct(*user.id), Federated, All),
                false => Err(custom("Error: Missing access token"))?,
            },
            other => {
                log::warn!("Request for nonexistent endpoint: `{}`", other);
                Err(custom("Error: Nonexistent endpoint"))?
            }
        })
    }
}
