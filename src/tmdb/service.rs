use crate::activitypub::actors::helpers::ACTOR_IMAGE_MAX_SIZE;
use crate::activitypub::fetcher::fetchers::fetch_file;
use crate::mastodon_api::oauth::utils::generate_access_token;
use crate::validators::users::validate_local_username;
use mitra_config::Instance;
use mitra_models::database::DatabaseClient;
use mitra_models::profiles::queries::update_profile;
use mitra_models::profiles::types::{ExtraField, ProfileImage, ProfileUpdateData};
use mitra_models::users::queries::create_user;
use mitra_models::users::types::{Role, User, UserCreateData};
use mitra_utils::crypto_rsa::{generate_rsa_key, serialize_private_key};
use mitra_utils::markdown::markdown_basic_to_html;
use mitra_utils::passwords::hash_password;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

pub async fn lookup_and_create_movie_user(
    instance: &Instance,
    db_client: &mut impl DatabaseClient,
    api_key: &str,
    media_dir: &Path,
    username: &str,
    default_movie_user_password: Option<String>,
) -> Result<User, MovieError> {
    let (movie_title, year) = username.rsplit_once('_').ok_or(MovieError::NotMovie)?;
    let year = year.parse::<u32>().map_err(|_| MovieError::NotMovie)?;

    let movie_info = get_movie_info(api_key, movie_title, year).await?;
    let user = create_movie_user(
        instance,
        db_client,
        &movie_info,
        &default_movie_user_password
            .clone()
            .unwrap_or_else(generate_access_token),
        media_dir,
    )
    .await?;

    Ok(user)
}

#[derive(thiserror::Error, Debug)]
pub enum MovieError {
    #[error("error calling TMDB API: {0}")]
    NotFoundError(&'static str),

    #[error("not a movie")]
    NotMovie,

    #[error("error creating movie user: {0}")]
    UserCreationError(#[from] anyhow::Error),
}

pub async fn get_movie_info(
    api_key: &str,
    movie_username: &str,
    year: u32,
) -> Result<MovieInfo, MovieError> {
    // Expand movie title to reverse the camel case on spaces
    let movie_title = movie_username
        .chars()
        .map(|c| {
            if c.is_uppercase() {
                format!(" {}", c)
            } else {
                c.to_string()
            }
        })
        .collect::<String>()
        .trim()
        .to_string();

    let client = reqwest::Client::new();
    let url = format!(
        "https://api.themoviedb.org/3/search/movie?api_key={api_key}&query={movie_title}&year={year}&include_adult=false&language=en-US",
    );
    let response = client
        .get(&url)
        .header("User-Agent", "FediMovies.rocks/1.0")
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| MovieError::NotFoundError("sending request"))?;
    let response = response
        .json::<MovieSearchResponse>()
        .await
        .map_err(|err| {
            log::error!("error parsing result from TMDB API ({url}): {err:?}");
            MovieError::NotFoundError("error parsing result from TMDB API")
        })?;
    let movie_info = response.results.first().ok_or_else(|| {
        log::error!("movie not found in TMDB API ({url})");
        MovieError::NotFoundError("movie not found in TMDB API")
    })?;

    if movie_info.movie_username().starts_with(movie_username)
        && movie_info.release_date.starts_with(&year.to_string())
        && !movie_info.adult
    {
        Ok(movie_info.clone())
    } else {
        let movie_username_gen = movie_info.movie_username();
        log::error!("does not movie match any movie found in TMDB API ({url}) - {movie_username_gen} != {movie_username}");
        Err(MovieError::NotFoundError(
            "does not movie match any movie found in TMDB API",
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MovieSearchResponse {
    pub page: u32,
    pub results: Vec<MovieInfo>,
    pub total_results: u32,
    pub total_pages: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MovieInfo {
    pub poster_path: Option<String>,
    pub adult: bool,
    pub overview: String,
    pub release_date: String,
    pub id: u32,
    pub original_title: String,
    pub original_language: String,
    pub title: String,
    pub backdrop_path: Option<String>,
    pub popularity: f32,
    pub vote_count: u32,
    pub video: bool,
    pub vote_average: f32,
}

impl MovieInfo {
    pub fn movie_username(&self) -> String {
        // Sanitize the movie title by removing all non-alphanumeric characters and replacing
        // space with camel case letter.
        let title = self
            .title
            .replace(|c: char| !c.is_alphanumeric() && !c.is_whitespace(), "");
        let title = title
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first_char) => first_char.to_uppercase().chain(chars).collect(),
                }
            })
            .collect::<Vec<String>>()
            .join("");
        format!("{}_{}", title, self.release_date.split('-').next().unwrap())
    }

    pub fn poster_url(&self) -> Option<String> {
        self.poster_path
            .as_ref()
            .map(|path| format!("https://image.tmdb.org/t/p/w185{}", path))
    }

    pub fn background_url(&self) -> Option<String> {
        self.backdrop_path
            .as_ref()
            .map(|path| format!("https://image.tmdb.org/t/p/w780{}", path))
    }

    pub fn movie_url(&self) -> String {
        format!("https://www.themoviedb.org/movie/{}", self.id)
    }
}

pub async fn create_movie_user(
    instance: &Instance,
    db_client: &mut impl DatabaseClient,
    movie_info: &MovieInfo,
    password: &str,
    media_dir: &Path,
) -> Result<User, anyhow::Error> {
    let username = movie_info.movie_username();
    validate_local_username(&username)?;
    let password_hash = hash_password(password)?;
    let private_key = generate_rsa_key()?;
    let private_key_pem = serialize_private_key(&private_key)?;
    let user_data = UserCreateData {
        username: username.clone(),
        password_hash: Some(password_hash),
        private_key_pem,
        wallet_address: None,
        invite_code: None,
        role: Role::NormalUser,
    };
    let mut user = create_user(db_client, user_data).await?;
    log::info!("user {username} created");
    // Update profile
    let mut profile_data = ProfileUpdateData::from(&user.profile);

    let tmdb_profile_url = movie_info.movie_url();
    profile_data.extra_fields = vec![ExtraField {
        name: "TMDB Profile".to_string(),
        value: markdown_basic_to_html(&tmdb_profile_url)
            .unwrap_or_else(|_| tmdb_profile_url.clone()),
        value_source: Some(tmdb_profile_url),
    }];
    profile_data.bio = Some(movie_info.overview.clone());
    profile_data.display_name = Some(movie_info.title.clone());
    match movie_info.poster_url() {
        Some(poster_url) => {
            profile_data.avatar =
                match fetch_file(instance, &poster_url, None, ACTOR_IMAGE_MAX_SIZE, media_dir).await
                {
                    Ok((file_name, file_size, maybe_media_type)) => {
                        let image = ProfileImage::new(file_name, file_size, maybe_media_type);
                        Some(image)
                    }
                    Err(error) => {
                        log::warn!("failed to fetch movie poster ({})", error);
                        None
                    }
                }
        }
        None => profile_data.avatar = None,
    }
    match movie_info.background_url() {
        Some(background_url) => {
            profile_data.banner = match fetch_file(
                instance,
                &background_url,
                None,
                ACTOR_IMAGE_MAX_SIZE,
                media_dir,
            )
            .await
            {
                Ok((file_name, file_size, maybe_media_type)) => {
                    let image = ProfileImage::new(file_name, file_size, maybe_media_type);
                    Some(image)
                }
                Err(error) => {
                    log::warn!("failed to fetch movie background ({})", error);
                    None
                }
            }
        }
        None => profile_data.banner = None,
    }
    user.profile = update_profile(db_client, &user.id, profile_data).await?;
    log::info!("user {username} profile updated");
    Ok(user)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_movie_username() {
        let movie = MovieInfo {
            poster_path: None,
            adult: false,
            overview: String::new(),
            release_date: String::from("2022-01-01"),
            id: 0,
            original_title: String::new(),
            original_language: String::new(),
            title: String::from("Avatar: The Way of Water"),
            backdrop_path: None,
            popularity: 0.0,
            vote_count: 0,
            video: false,
            vote_average: 0.0,
        };

        assert_eq!(movie.movie_username(), "AvatarTheWayOfWater_2022");
    }
}
