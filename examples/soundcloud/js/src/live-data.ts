// Spike 7b: JS-side wrapper over the `__sc*` host functions that bridge to
// `sc-rn` (Core/shared's uniffi crate — see js-host/src/host.rs and
// live_data.rs). Every async call gets its own callback id and a pending
// Promise; the Rust render loop resolves/rejects it once per frame by
// calling `__scDeliverResult(callbackId, ok, payload)` (rn-linux calls
// `live_data::deliver()` right alongside the reanimated tick).

type Pending = { resolve: (value: any) => void; reject: (error: Error) => void };

const pending = new Map<number, Pending>();
let nextCallbackId = 1;

(globalThis as any).__scDeliverResult = function (callbackId: number, ok: boolean, payload: any) {
  const p = pending.get(callbackId);
  if (!p) return;
  pending.delete(callbackId);
  if (ok) p.resolve(payload);
  else p.reject(new Error(String(payload)));
};

function callAsync<T>(invoke: (callbackId: number) => void): Promise<T> {
  const id = nextCallbackId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve, reject });
    invoke(id);
  });
}

declare function __scInitCore(dataDir: string, cacheDir: string, dpiBypass: boolean): string;
declare function __scSetSession(token: string): string;
declare function __scAuthStatus(callbackId: number): void;
declare function __scMe(callbackId: number): void;
declare function __scHomeClusters(callbackId: number, limit: number, languagesJson: string, hideListened: boolean): void;
declare function __scWave(callbackId: number, limit: number, cursor: string, languagesJson: string, hideListened: boolean): void;
declare function __scResolveTracks(callbackId: number, urnsJson: string): void;

export interface ArtistRef {
  id: string;
  name: string;
  avatarUrl: string | null;
}

export interface UserRef {
  id: string;
  username: string;
  permalink: string | null;
  permalinkUrl: string | null;
  avatarUrl: string | null;
  verified: boolean;
}

export interface TrackAlbum {
  id: string;
  title: string;
  coverUrl: string | null;
  year: number | null;
}

export interface TrackParticipant {
  id: string | null;
  name: string;
  role: string | null;
}

export interface TrackBadge {
  storageState: string | null;
  storageQuality: string | null;
  indexState: string | null;
  enrichState: string | null;
}

export interface Track {
  id: string;
  title: string;
  artist: ArtistRef;
  durationMs: number;
  artworkUrl: string | null;
  waveformUrl: string | null;
  genre: string | null;
  playCount: number | null;
  likesCount: number | null;
  repostsCount: number | null;
  permalinkUrl: string | null;
  createdAt: string | null;
  releaseYear: number | null;
  uploader: UserRef | null;
  badge: TrackBadge;
  userFavorite: boolean | null;
  isCover: boolean;
  description: string | null;
  tags: string[];
  isrc: string | null;
  language: string | null;
  album: TrackAlbum | null;
  participants: TrackParticipant[];
}

export interface ClusterNeighbor {
  artistId: string;
  artistName: string;
  avatarUrl: string | null;
  trackId: string;
}

export interface Cluster {
  id: string;
  trackIds: string[];
  neighbors: ClusterNeighbor[];
}

export interface WaveItem {
  id: number;
  score: number;
}

export interface Wave {
  items: WaveItem[];
  cursor: string | null;
}

export interface Me {
  id: string;
  username: string;
  permalink: string | null;
  permalinkUrl: string | null;
  avatarUrl: string | null;
  plan: string | null;
  premium: boolean;
  followersCount: number | null;
  followingsCount: number | null;
  publicFavoritesCount: number | null;
  privatePlaylistsCount: number | null;
  playlistCount: number | null;
}

export interface AuthStatus {
  hasSession: boolean;
  authenticated: boolean;
  sessionId: string | null;
  username: string | null;
  tokenState: string | null;
}

export function initCore(dataDir: string, cacheDir: string, dpiBypass: boolean): void {
  const err = __scInitCore(dataDir, cacheDir, dpiBypass);
  if (err) throw new Error(`sc-rn init_runtime failed: ${err}`);
}

export function setSession(token: string | null): void {
  const err = __scSetSession(token ?? '');
  if (err) throw new Error(`sc-rn set_session failed: ${err}`);
}

export function authStatus(): Promise<AuthStatus> {
  return callAsync<AuthStatus>((id) => __scAuthStatus(id));
}

export function me(): Promise<Me> {
  return callAsync<Me>((id) => __scMe(id));
}

export function homeClusters(limit: number, languages: string[] = [], hideListened = false): Promise<Cluster[]> {
  return callAsync<Cluster[]>((id) => __scHomeClusters(id, limit, JSON.stringify(languages), hideListened));
}

export function wave(
  limit: number,
  cursor: string | null = null,
  languages: string[] = [],
  hideListened = false,
): Promise<Wave> {
  return callAsync<Wave>((id) => __scWave(id, limit, cursor ?? '', JSON.stringify(languages), hideListened));
}

export function resolveTracks(urns: string[]): Promise<Track[]> {
  return callAsync<Track[]>((id) => __scResolveTracks(id, JSON.stringify(urns)));
}
