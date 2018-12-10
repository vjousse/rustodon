use chrono::offset::Utc;
use db::models::{Account, NewStatus, Status, User};
use db::{self, id_generator};
use error::Perhaps;
use failure::Error;
use itertools::Itertools;
use rocket::http::RawStr;
use rocket::request::{FlashMessage, Form, FromFormValue};
use rocket::response::{Flash, NamedFile, Redirect};
use rocket::Route;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use util::Either;
use validator::Validate;

#[macro_use]
mod templates;
mod auth;
pub mod view_helpers;

use self::templates::*;

pub fn routes() -> Vec<Route> {
    routes![
        index,
        user_page,
        settings_profile,
        settings_profile_update,
        status_page,
        create_status,
        auth::signin_get,
        auth::signin_post,
        auth::signout,
        auth::signup_get,
        auth::signup_post,
        static_files
    ]
}

#[derive(Debug)]
pub enum Timeline {
    Local,
    Federated,
}

impl<'v> FromFormValue<'v> for Timeline {
    type Error = &'v RawStr;

    fn from_form_value(form_value: &'v RawStr) -> Result<Self, Self::Error> {
        match form_value.as_str() {
            "local" => Ok(Timeline::Local),
            "federated" => Ok(Timeline::Federated),
            _ => Err(form_value),
        }
    }
}

#[derive(Debug, FromForm, Validate)]
pub struct CreateStatusForm {
    #[validate(length(min = "1", message = "Content must not be empty"))]
    content: String,
    content_warning: String,
}

#[post("/statuses/create", data = "<form>")]
pub fn create_status(
    user: User,
    db_conn: db::Connection,
    form: Form<CreateStatusForm>,
) -> Result<Either<Flash<Redirect>, Redirect>, Error> {
    if let Err(errs) = form.validate() {
        let errs = errs.field_errors();

        // concatenate the error descriptions, with commas between them.
        // TODO: make this less ugly :(
        let error_desc = errs
            .iter()
            .flat_map(|(_, errs)| errs)
            .map(|e| {
                let msg = e.message.to_owned();
                msg.unwrap_or(Cow::Borrowed("unknown error"))
            })
            .join(", ");

        return Ok(Either::Left(Flash::error(Redirect::to("/"), error_desc)));
    }

    // convert CW to option if present, so we get proper nulls in DB
    let content_warning: Option<String> = if !form.content_warning.is_empty() {
        Some(form.content_warning.to_owned())
    } else {
        None
    };

    let _status = NewStatus {
        id: id_generator().next(),
        created_at: Utc::now(),
        text: form.content.to_owned(),
        content_warning,
        account_id: user.account_id,
    }
    .insert(&db_conn)?;

    Ok(Either::Right(Redirect::to("/")))
}

#[get("/users/<username>/statuses/<status_id>", format = "text/html")]
pub fn status_page<'b, 'c>(
    username: String,
    status_id: u64,
    db_conn: db::Connection,
) -> Perhaps<StatusTemplate<'static, 'b, 'c>> {
    let account = try_resopt!(Account::fetch_local_by_username(&db_conn, username));
    let status = try_resopt!(Status::by_account_and_id(
        &db_conn,
        account.id,
        status_id as i64
    ));

    PerhapsHtmlTemplate!(StatusTemplate, {
        status:  status,
        account: account,
        connection: db_conn
    })
}


#[get("/users/<username>?<max_id>", format = "text/html")]
pub fn user_page<'b, 'c>(
    username: String,
    max_id: Option<i64>,
    db_conn: db::Connection,
    account: Option<Account>,
) -> Perhaps<UserTemplate<'static, 'b, 'c>> {
    let account_to_show = try_resopt!(Account::fetch_local_by_username(&db_conn, username));
    let statuses: Vec<Status> = account_to_show.statuses_before_id(&db_conn, max_id, 10)?;
    let prev_page_id = if let Some(prev_page_max_id) = statuses.iter().map(|s| s.id).min() {
        let bounds = account_to_show.status_id_bounds(&db_conn)?;
        // unwrap is safe since we already know we have statuses
        if prev_page_max_id > bounds.unwrap().0 {
            Some(prev_page_max_id)
        } else {
            None
        }
    } else {
        None
    };
    PerhapsHtmlTemplate!(UserTemplate, {
        account_to_show: account_to_show,
        account: account,
        statuses: statuses,
        prev_page_id: prev_page_id,
        connection: db_conn
    })
}

#[get("/settings/profile")]
pub fn settings_profile<'b, 'c>(
    db_conn: db::Connection,
    user: User,
) -> Perhaps<EditProfileTemplate<'static, 'b, 'c>> {
    PerhapsHtmlTemplate!(EditProfileTemplate, {
        account: user.get_account(&db_conn)?
    })
}

#[derive(Debug, FromForm)]
pub struct UpdateProfileForm {
    summary: String,
}

#[post("/settings/profile", data = "<form>")]
pub fn settings_profile_update(
    db_conn: db::Connection,
    user: User,
    form: Form<UpdateProfileForm>,
) -> Result<Redirect, Error> {
    let account = user.get_account(&db_conn)?;

    // `as &str` defeat an incorrect deref coercion (due to the second match arm)
    let new_summary = match &form.summary as &str {
        "" => None,
        x => Some(x.to_string()),
    };
    account.set_summary(&db_conn, new_summary)?;

    Ok(Redirect::to(account.profile_path().to_string()))
}

#[get("/?<max_id>&<timeline>")]
pub fn index<'b, 'c>(
    flash: Option<FlashMessage<'b, 'c>>,
    account: Option<Account>,
    max_id:   Option<i64>,
    timeline: Option<Timeline>,
    db_conn: db::Connection,
) -> Result<IndexTemplate<'static, 'b, 'c>, Error> {
    let statuses: Vec<Status> = match timeline {
        Some(Timeline::Local) | None => Status::local_before_id(&db_conn, max_id, 10)?,
        Some(Timeline::Federated) => Status::federated_before_id(&db_conn, max_id, 10)?,
    };

    let prev_page_id = if let Some(prev_page_max_id) = statuses.iter().map(|s| s.id).min() {
        let bounds = match timeline {
            Some(Timeline::Local) | None => Status::local_status_id_bounds(&db_conn)?,
            Some(Timeline::Federated) => Status::federated_status_id_bounds(&db_conn)?,
        };
        // unwrap is safe since we already know we have statuses
        if prev_page_max_id > bounds.unwrap().0 {
            Some(prev_page_max_id)
        } else {
            None
        }
    } else {
        None
    };

    // todo: Into<String> and/or localization
    let timeline_str = match timeline {
        Some(Timeline::Local) | None => "local",
        Some(Timeline::Federated) => "federated",
    };

    Ok(HtmlTemplate!(IndexTemplate, flash, {
        account: account,
        statuses: statuses,
        timeline: timeline_str,
        prev_page_id: prev_page_id,
        connection: db_conn
    }))
}

#[get("/static/<path..>")]
fn static_files(path: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(path)).ok()
}
