//! `sc-rn`'s DTOs derive `uniffi::Record`, not `serde::Serialize` (they're
//! meant for uniffi's own codegen, not us calling them directly from Rust) —
//! these are plain field-by-field JSON builders instead of pulling `serde`
//! into `Core/shared/crates/sc-rn` just for our sake.

use serde_json::{Value as Json, json};

pub fn auth_status(dto: &sc_rn::AuthStatusDto) -> Json {
    json!({
        "hasSession": dto.has_session,
        "authenticated": dto.authenticated,
        "sessionId": dto.session_id,
        "username": dto.username,
        "tokenState": dto.token_state,
    })
}

pub fn me(dto: &sc_rn::MeDto) -> Json {
    json!({
        "id": dto.id,
        "username": dto.username,
        "permalink": dto.permalink,
        "permalinkUrl": dto.permalink_url,
        "avatarUrl": dto.avatar_url,
        "plan": dto.plan,
        "premium": dto.premium,
        "followersCount": dto.followers_count,
        "followingsCount": dto.followings_count,
        "publicFavoritesCount": dto.public_favorites_count,
        "privatePlaylistsCount": dto.private_playlists_count,
        "playlistCount": dto.playlist_count,
    })
}

pub fn cluster(dto: &sc_rn::ClusterDto) -> Json {
    json!({
        "id": dto.id,
        "trackIds": dto.track_ids,
        "neighbors": dto.neighbors.iter().map(cluster_neighbor).collect::<Vec<_>>(),
    })
}

fn cluster_neighbor(dto: &sc_rn::ClusterNeighborDto) -> Json {
    json!({
        "artistId": dto.artist_id,
        "artistName": dto.artist_name,
        "avatarUrl": dto.avatar_url,
        "trackId": dto.track_id,
    })
}

pub fn wave(dto: &sc_rn::WaveDto) -> Json {
    json!({
        "items": dto.items.iter().map(|i| json!({ "id": i.id, "score": i.score })).collect::<Vec<_>>(),
        "cursor": dto.cursor,
    })
}

pub fn track(dto: &sc_rn::TrackDto) -> Json {
    json!({
        "id": dto.id,
        "title": dto.title,
        "artist": artist_ref(&dto.artist),
        "durationMs": dto.duration_ms,
        "artworkUrl": dto.artwork_url,
        "waveformUrl": dto.waveform_url,
        "genre": dto.genre,
        "playCount": dto.play_count,
        "likesCount": dto.likes_count,
        "repostsCount": dto.reposts_count,
        "permalinkUrl": dto.permalink_url,
        "createdAt": dto.created_at,
        "releaseYear": dto.release_year,
        "uploader": dto.uploader.as_ref().map(user_ref),
        "badge": track_badge(&dto.badge),
        "userFavorite": dto.user_favorite,
        "isCover": dto.is_cover,
        "description": dto.description,
        "tags": dto.tags,
        "isrc": dto.isrc,
        "language": dto.language,
        "album": dto.album.as_ref().map(track_album),
        "participants": dto.participants.iter().map(track_participant).collect::<Vec<_>>(),
    })
}

fn artist_ref(dto: &sc_rn::ArtistRefDto) -> Json {
    json!({ "id": dto.id, "name": dto.name, "avatarUrl": dto.avatar_url })
}

fn user_ref(dto: &sc_rn::UserRefDto) -> Json {
    json!({
        "id": dto.id,
        "username": dto.username,
        "permalink": dto.permalink,
        "permalinkUrl": dto.permalink_url,
        "avatarUrl": dto.avatar_url,
        "verified": dto.verified,
    })
}

fn track_album(dto: &sc_rn::TrackAlbumDto) -> Json {
    json!({ "id": dto.id, "title": dto.title, "coverUrl": dto.cover_url, "year": dto.year })
}

fn track_participant(dto: &sc_rn::TrackParticipantDto) -> Json {
    json!({ "id": dto.id, "name": dto.name, "role": dto.role })
}

fn track_badge(dto: &sc_rn::TrackBadgeDto) -> Json {
    json!({
        "storageState": dto.storage_state,
        "storageQuality": dto.storage_quality,
        "indexState": dto.index_state,
        "enrichState": dto.enrich_state,
    })
}
