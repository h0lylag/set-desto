use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[cfg(test)]
use std::io::Cursor;

use anyhow::{Context, Result, anyhow, bail};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use zip::ZipArchive;

use crate::storage::config::cache_dir;

pub const MAX_OPTIMIZED_ROUTE_STOPS: usize = 18;

const SDE_METADATA_URL: &str =
    "https://developers.eveonline.com/static-data/tranquility/latest.jsonl";
const SDE_JSONL_ZIP_URL: &str =
    "https://developers.eveonline.com/static-data/eve-online-static-data-latest-jsonl.zip";
const ROUTE_CACHE_FILE: &str = "route-graph.json";
const SDE_METADATA_CACHE_FILE: &str = "sde-metadata.json";
const SOLAR_SYSTEMS_CACHE_FILE: &str = "mapSolarSystems.jsonl";
const STARGATES_CACHE_FILE: &str = "mapStargates.jsonl";
const SDE_ZIP_FILE: &str = "eve-online-static-data-latest-jsonl.zip";

#[derive(Clone, Debug)]
pub struct RouteGraph {
    build_number: u64,
    release_date: String,
    systems: HashMap<i64, SolarSystem>,
    name_lookup: HashMap<String, i64>,
    adjacency: HashMap<i64, Vec<i64>>,
}

impl RouteGraph {
    pub fn build_number(&self) -> u64 {
        self.build_number
    }

    pub fn release_date(&self) -> &str {
        &self.release_date
    }

    pub fn system_count(&self) -> usize {
        self.systems.len()
    }

    pub fn edge_count(&self) -> usize {
        self.adjacency.values().map(Vec::len).sum()
    }

    pub fn resolve_system(&self, name: &str) -> Option<RouteSystem> {
        let key = normalize_system_name(name);
        let system_id = *self.name_lookup.get(&key)?;
        self.system(system_id)
    }

    pub fn system(&self, system_id: i64) -> Option<RouteSystem> {
        self.systems.get(&system_id).map(|system| RouteSystem {
            id: system.id,
            name: system.name.clone(),
            security_status: system.security_status,
        })
    }

    #[cfg(test)]
    pub fn distance(&self, origin: i64, destination: i64) -> Option<usize> {
        self.distances_from(origin).get(&destination).copied()
    }

    pub fn distances_from(&self, origin: i64) -> HashMap<i64, usize> {
        let mut distances = HashMap::new();
        if !self.systems.contains_key(&origin) {
            return distances;
        }

        let mut queue = BinaryHeap::new();
        distances.insert(origin, 0);
        queue.push(Reverse(QueuedSystem {
            distance: 0,
            system_id: origin,
        }));

        while let Some(Reverse(QueuedSystem {
            distance,
            system_id,
        })) = queue.pop()
        {
            if distances
                .get(&system_id)
                .is_some_and(|known| distance > *known)
            {
                continue;
            }

            for neighbor in self
                .adjacency
                .get(&system_id)
                .into_iter()
                .flat_map(|neighbors| neighbors.iter())
            {
                let next_distance = distance + 1;
                if distances
                    .get(neighbor)
                    .is_none_or(|known| next_distance < *known)
                {
                    distances.insert(*neighbor, next_distance);
                    queue.push(Reverse(QueuedSystem {
                        distance: next_distance,
                        system_id: *neighbor,
                    }));
                }
            }
        }

        distances
    }

    pub fn optimize_open_route(
        &self,
        origin: i64,
        destinations: &[RouteDestination],
    ) -> Result<OptimizedRoute> {
        if destinations.is_empty() {
            bail!("Select at least one imported system");
        }

        if destinations.len() > MAX_OPTIMIZED_ROUTE_STOPS {
            bail!(
                "Select {MAX_OPTIMIZED_ROUTE_STOPS} or fewer systems for exact optimized routing"
            );
        }

        let origin_system = self
            .system(origin)
            .ok_or_else(|| anyhow!("Current solar system {origin} was not found in SDE cache"))?;
        let mut distances = self.route_distances(origin, destinations)?;
        let order = exact_open_route_order(&mut distances)?;
        let ordered_destinations = order
            .into_iter()
            .map(|index| destinations[index].clone())
            .collect();

        Ok(OptimizedRoute {
            origin: origin_system,
            destinations: ordered_destinations,
            total_jumps: distances.best_total_jumps,
        })
    }

    fn route_distances(
        &self,
        origin: i64,
        destinations: &[RouteDestination],
    ) -> Result<RouteDistances> {
        let origin_distances = self.distances_from(origin);
        let mut origin_to_destination = Vec::with_capacity(destinations.len());
        for destination in destinations {
            let distance = origin_distances
                .get(&destination.system_id)
                .copied()
                .ok_or_else(|| {
                    anyhow!(
                        "No stargate route from current system to {}",
                        destination.system_name
                    )
                })?;
            origin_to_destination.push(distance);
        }

        let mut destination_to_destination = vec![vec![0; destinations.len()]; destinations.len()];
        for (left_index, left) in destinations.iter().enumerate() {
            let distances = self.distances_from(left.system_id);
            for (right_index, right) in destinations.iter().enumerate() {
                if left_index == right_index {
                    continue;
                }
                destination_to_destination[left_index][right_index] =
                    distances.get(&right.system_id).copied().ok_or_else(|| {
                        anyhow!(
                            "No stargate route from {} to {}",
                            left.system_name,
                            right.system_name
                        )
                    })?;
            }
        }

        Ok(RouteDistances {
            origin_to_destination,
            destination_to_destination,
            best_total_jumps: 0,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RouteSystem {
    pub id: i64,
    pub name: String,
    pub security_status: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteDestination {
    pub system_id: i64,
    pub system_name: String,
    pub import_index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OptimizedRoute {
    pub origin: RouteSystem,
    pub destinations: Vec<RouteDestination>,
    pub total_jumps: usize,
}

#[derive(Debug)]
pub enum SdeCacheEvent {
    Status {
        message: String,
    },
    Ready {
        graph: Arc<RouteGraph>,
        refreshed: bool,
        message: String,
    },
    Failed {
        error: String,
    },
}

pub fn load_cached_graph() -> Result<Arc<RouteGraph>> {
    let cache_path = route_cache_path()?;
    let contents = fs::read_to_string(&cache_path).with_context(|| {
        format!(
            "Failed to read SDE route cache from {}",
            cache_path.display()
        )
    })?;
    let cache: CachedRouteGraph = serde_json::from_str(&contents).with_context(|| {
        format!(
            "Failed to parse SDE route cache from {}",
            cache_path.display()
        )
    })?;

    Ok(Arc::new(RouteGraph::from_cache(cache)))
}

pub fn start_cache_refresh(cached_build_number: Option<u64>) -> Receiver<SdeCacheEvent> {
    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || {
        let event = match refresh_route_cache(cached_build_number, &sender) {
            Ok(Some(graph)) => SdeCacheEvent::Ready {
                graph,
                refreshed: true,
                message: "updated".to_string(),
            },
            Ok(None) => match load_cached_graph() {
                Ok(graph) => SdeCacheEvent::Ready {
                    graph,
                    refreshed: false,
                    message: "current".to_string(),
                },
                Err(err) => SdeCacheEvent::Failed {
                    error: err.to_string(),
                },
            },
            Err(err) => SdeCacheEvent::Failed {
                error: err.to_string(),
            },
        };

        let _ = sender.send(event);
    });

    receiver
}

fn refresh_route_cache(
    cached_build_number: Option<u64>,
    sender: &mpsc::Sender<SdeCacheEvent>,
) -> Result<Option<Arc<RouteGraph>>> {
    send_status(sender, "checking for updates");
    let metadata = fetch_sde_metadata()?;
    debug!(
        build_number = metadata.build_number,
        release_date = %metadata.release_date,
        "Fetched SDE metadata"
    );

    if cached_build_number == Some(metadata.build_number) {
        return Ok(None);
    }

    let cache_directory = cache_dir()?;
    fs::create_dir_all(&cache_directory).with_context(|| {
        format!(
            "Failed to create cache directory {}",
            cache_directory.display()
        )
    })?;
    let zip_path = cache_directory.join(SDE_ZIP_FILE);

    remove_file_if_exists(&zip_path)?;
    send_status(sender, "downloading route map");
    download_sde_zip(&zip_path)?;
    send_status(sender, "extracting route data");
    extract_needed_jsonl_files(&zip_path, &cache_directory)?;
    remove_file_if_exists(&zip_path)?;
    send_status(sender, "building route map");
    let cache = parse_sde_cache_files(&cache_directory, metadata)?;
    send_status(sender, "saving route map");
    save_route_cache(&cache)?;
    save_sde_metadata(&cache)?;
    cleanup_stale_sde_artifacts(&cache_directory)?;

    Ok(Some(Arc::new(RouteGraph::from_cache(cache))))
}

fn send_status(sender: &mpsc::Sender<SdeCacheEvent>, message: &str) {
    let _ = sender.send(SdeCacheEvent::Status {
        message: message.to_string(),
    });
}

fn fetch_sde_metadata() -> Result<SdeMetadata> {
    let client = Client::builder()
        .user_agent(format!("set-desto/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("Failed to build SDE metadata HTTP client")?;
    let response = client
        .get(SDE_METADATA_URL)
        .send()
        .context("Failed to fetch SDE metadata")?;

    if response.status() != StatusCode::OK {
        bail!(
            "SDE metadata fetch failed ({}): {}",
            response.status(),
            response.text()?
        );
    }

    response.json().context("Failed to parse SDE metadata")
}

fn download_sde_zip(zip_path: &Path) -> Result<()> {
    let client = Client::builder()
        .user_agent(format!("set-desto/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .context("Failed to build SDE download HTTP client")?;
    let mut response = client
        .get(SDE_JSONL_ZIP_URL)
        .send()
        .context("Failed to download SDE JSONL zip")?;

    if response.status() != StatusCode::OK {
        bail!("SDE JSONL zip download failed ({})", response.status());
    }

    let temp_path = zip_path.with_extension("zip.tmp");
    let mut file = File::create(&temp_path)
        .with_context(|| format!("Failed to create {}", temp_path.display()))?;
    std::io::copy(&mut response, &mut file).context("Failed to write SDE JSONL zip")?;
    fs::rename(&temp_path, zip_path).with_context(|| {
        format!(
            "Failed to move SDE JSONL zip from {} to {}",
            temp_path.display(),
            zip_path.display()
        )
    })?;

    Ok(())
}

fn extract_needed_jsonl_files(zip_path: &Path, cache_directory: &Path) -> Result<()> {
    let file = File::open(zip_path)
        .with_context(|| format!("Failed to open SDE JSONL zip {}", zip_path.display()))?;
    let mut archive = ZipArchive::new(file).context("Failed to read SDE JSONL zip")?;
    extract_zip_entry(
        &mut archive,
        SOLAR_SYSTEMS_CACHE_FILE,
        &cache_directory.join(SOLAR_SYSTEMS_CACHE_FILE),
    )?;
    extract_zip_entry(
        &mut archive,
        STARGATES_CACHE_FILE,
        &cache_directory.join(STARGATES_CACHE_FILE),
    )?;

    Ok(())
}

fn parse_sde_cache_files(
    cache_directory: &Path,
    metadata: SdeMetadata,
) -> Result<CachedRouteGraph> {
    let systems = parse_solar_systems(&cache_directory.join(SOLAR_SYSTEMS_CACHE_FILE))?;
    let edges = parse_stargates(&cache_directory.join(STARGATES_CACHE_FILE))?;

    info!(
        build_number = metadata.build_number,
        system_count = systems.len(),
        edge_count = edges.len(),
        "Parsed SDE route graph"
    );

    Ok(CachedRouteGraph {
        build_number: metadata.build_number,
        release_date: metadata.release_date,
        systems,
        edges,
    })
}

fn parse_solar_systems(path: &Path) -> Result<Vec<SolarSystem>> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open SDE solar systems file {}", path.display()))?;
    let reader = BufReader::new(file);

    parse_solar_system_reader(reader)
}

fn parse_solar_system_reader(reader: impl BufRead) -> Result<Vec<SolarSystem>> {
    let mut systems = Vec::new();

    for (line_number, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!(
                "Failed to read mapSolarSystems.jsonl line {}",
                line_number + 1
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let raw: RawSolarSystem = serde_json::from_str(&line).with_context(|| {
            format!(
                "Failed to parse mapSolarSystems.jsonl line {}",
                line_number + 1
            )
        })?;
        systems.push(SolarSystem {
            id: raw.solar_system_id,
            name: raw.name.en,
            security_status: raw.security_status,
        });
    }

    Ok(systems)
}

fn parse_stargates(path: &Path) -> Result<Vec<RouteEdge>> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open SDE stargates file {}", path.display()))?;
    let reader = BufReader::new(file);

    parse_stargate_reader(reader)
}

fn parse_stargate_reader(reader: impl BufRead) -> Result<Vec<RouteEdge>> {
    let mut edges = Vec::new();

    for (line_number, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!("Failed to read mapStargates.jsonl line {}", line_number + 1)
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let raw: RawStargate = serde_json::from_str(&line).with_context(|| {
            format!(
                "Failed to parse mapStargates.jsonl line {}",
                line_number + 1
            )
        })?;
        let Some(destination) = raw.destination else {
            warn!(
                stargate_id = raw.stargate_id,
                "Skipping stargate without destination"
            );
            continue;
        };
        edges.push(RouteEdge {
            from: raw.solar_system_id,
            to: destination.solar_system_id,
        });
    }

    Ok(edges)
}

fn extract_zip_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    expected_suffix: &str,
    output_path: &Path,
) -> Result<()> {
    let index = (0..archive.len())
        .find(|index| {
            archive
                .by_index(*index)
                .map(|file| file.name().ends_with(expected_suffix))
                .unwrap_or(false)
        })
        .ok_or_else(|| anyhow!("SDE JSONL zip did not include {expected_suffix}"))?;

    let mut file = archive
        .by_index(index)
        .with_context(|| format!("Failed to open {expected_suffix} from SDE JSONL zip"))?;
    let temp_path = output_path.with_extension("jsonl.tmp");
    let mut output = File::create(&temp_path)
        .with_context(|| format!("Failed to create {}", temp_path.display()))?;
    std::io::copy(&mut file, &mut output)
        .with_context(|| format!("Failed to extract {expected_suffix}"))?;
    fs::rename(&temp_path, output_path).with_context(|| {
        format!(
            "Failed to move extracted {expected_suffix} from {} to {}",
            temp_path.display(),
            output_path.display()
        )
    })?;

    Ok(())
}

fn save_route_cache(cache: &CachedRouteGraph) -> Result<()> {
    let path = route_cache_path()?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("SDE route cache path did not include a parent directory"))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create cache directory {}", parent.display()))?;

    let contents = serde_json::to_string(cache).context("Failed to serialize SDE route cache")?;
    fs::write(&path, contents)
        .with_context(|| format!("Failed to write SDE route cache to {}", path.display()))?;

    Ok(())
}

fn save_sde_metadata(cache: &CachedRouteGraph) -> Result<()> {
    let path = cache_dir()?.join(SDE_METADATA_CACHE_FILE);
    let metadata = CachedSdeMetadata {
        build_number: cache.build_number,
        release_date: cache.release_date.clone(),
    };
    let contents = serde_json::to_string_pretty(&metadata)
        .context("Failed to serialize SDE metadata cache")?;
    fs::write(&path, contents)
        .with_context(|| format!("Failed to write SDE metadata cache to {}", path.display()))
}

fn cleanup_stale_sde_artifacts(cache_directory: &Path) -> Result<()> {
    remove_file_if_exists(&cache_directory.join(SDE_ZIP_FILE))?;

    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("Failed to remove {}", path.display())),
    }
}

fn route_cache_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join(ROUTE_CACHE_FILE))
}

fn normalize_system_name(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

fn exact_open_route_order(distances: &mut RouteDistances) -> Result<Vec<usize>> {
    let destination_count = distances.origin_to_destination.len();
    if destination_count == 0 {
        bail!("Select at least one imported system");
    }

    let state_count = 1_usize << destination_count;
    let current_count = destination_count + 1;
    let origin_index = destination_count;
    let full_mask = state_count - 1;
    let mut best_suffix_costs = vec![OPTIMIZER_UNKNOWN; state_count * current_count];
    let best_total = best_route_suffix_cost(
        origin_index,
        full_mask,
        distances,
        current_count,
        &mut best_suffix_costs,
    )?;
    if best_total == OPTIMIZER_UNREACHABLE {
        bail!("Could not optimize route");
    }

    distances.best_total_jumps = best_total as usize;

    let mut order = Vec::with_capacity(destination_count);
    let mut current_index = origin_index;
    let mut remaining_mask = full_mask;
    while remaining_mask != 0 {
        let current_best = best_route_suffix_cost(
            current_index,
            remaining_mask,
            distances,
            current_count,
            &mut best_suffix_costs,
        )?;
        let mut selected_next = None;

        for next_index in 0..destination_count {
            let next_bit = 1_usize << next_index;
            if remaining_mask & next_bit == 0 {
                continue;
            }

            let next_remaining_mask = remaining_mask & !next_bit;
            let suffix_cost = best_route_suffix_cost(
                next_index,
                next_remaining_mask,
                distances,
                current_count,
                &mut best_suffix_costs,
            )?;
            if suffix_cost == OPTIMIZER_UNREACHABLE {
                continue;
            }

            let total_cost = optimizer_sum(
                optimizer_edge_distance(current_index, next_index, distances),
                suffix_cost,
            )?;
            if total_cost == current_best {
                selected_next = Some(next_index);
                break;
            }
        }

        let next_index =
            selected_next.ok_or_else(|| anyhow!("Could not reconstruct optimized route"))?;
        order.push(next_index);
        remaining_mask &= !(1_usize << next_index);
        current_index = next_index;
    }

    Ok(order)
}

fn best_route_suffix_cost(
    current_index: usize,
    remaining_mask: usize,
    distances: &RouteDistances,
    current_count: usize,
    best_suffix_costs: &mut [u16],
) -> Result<u16> {
    if remaining_mask == 0 {
        return Ok(0);
    }

    let state_index = optimizer_state_index(remaining_mask, current_index, current_count);
    let cached = best_suffix_costs[state_index];
    if cached != OPTIMIZER_UNKNOWN {
        return Ok(cached);
    }

    let destination_count = distances.origin_to_destination.len();
    let mut best_cost = OPTIMIZER_UNREACHABLE;
    for next_index in 0..destination_count {
        let next_bit = 1_usize << next_index;
        if remaining_mask & next_bit == 0 {
            continue;
        }

        let next_remaining_mask = remaining_mask & !next_bit;
        let suffix_cost = best_route_suffix_cost(
            next_index,
            next_remaining_mask,
            distances,
            current_count,
            best_suffix_costs,
        )?;
        if suffix_cost == OPTIMIZER_UNREACHABLE {
            continue;
        }

        let total_cost = optimizer_sum(
            optimizer_edge_distance(current_index, next_index, distances),
            suffix_cost,
        )?;
        if total_cost < best_cost {
            best_cost = total_cost;
        }
    }

    best_suffix_costs[state_index] = best_cost;
    Ok(best_cost)
}

fn optimizer_edge_distance(
    current_index: usize,
    next_index: usize,
    distances: &RouteDistances,
) -> usize {
    if current_index == distances.origin_to_destination.len() {
        distances.origin_to_destination[next_index]
    } else {
        distances.destination_to_destination[current_index][next_index]
    }
}

fn optimizer_sum(edge_distance: usize, suffix_cost: u16) -> Result<u16> {
    optimizer_cost(edge_distance + suffix_cost as usize)
}

fn optimizer_cost(cost: usize) -> Result<u16> {
    let cost = u16::try_from(cost).context("Route distance exceeded optimizer capacity")?;
    if cost >= OPTIMIZER_UNKNOWN {
        bail!("Route distance exceeded optimizer capacity");
    }

    Ok(cost)
}

fn optimizer_state_index(mask: usize, current_index: usize, current_count: usize) -> usize {
    mask * current_count + current_index
}

impl RouteGraph {
    fn from_cache(cache: CachedRouteGraph) -> Self {
        let systems: HashMap<i64, SolarSystem> = cache
            .systems
            .into_iter()
            .map(|system| (system.id, system))
            .collect();
        let name_lookup = systems
            .values()
            .map(|system| (normalize_system_name(&system.name), system.id))
            .collect();
        let mut adjacency: HashMap<i64, Vec<i64>> = HashMap::new();

        for edge in cache.edges {
            adjacency.entry(edge.from).or_default().push(edge.to);
            adjacency.entry(edge.to).or_default().push(edge.from);
        }

        for neighbors in adjacency.values_mut() {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        Self {
            build_number: cache.build_number,
            release_date: cache.release_date,
            systems,
            name_lookup,
            adjacency,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct QueuedSystem {
    distance: usize,
    system_id: i64,
}

impl Ord for QueuedSystem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance
            .cmp(&other.distance)
            .then_with(|| self.system_id.cmp(&other.system_id))
    }
}

impl PartialOrd for QueuedSystem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

const OPTIMIZER_UNREACHABLE: u16 = u16::MAX;
const OPTIMIZER_UNKNOWN: u16 = u16::MAX - 1;

#[derive(Debug)]
struct RouteDistances {
    origin_to_destination: Vec<usize>,
    destination_to_destination: Vec<Vec<usize>>,
    best_total_jumps: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CachedRouteGraph {
    build_number: u64,
    release_date: String,
    systems: Vec<SolarSystem>,
    edges: Vec<RouteEdge>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SolarSystem {
    id: i64,
    name: String,
    security_status: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RouteEdge {
    from: i64,
    to: i64,
}

#[derive(Debug, Deserialize)]
struct SdeMetadata {
    #[serde(rename = "buildNumber")]
    build_number: u64,
    #[serde(rename = "releaseDate")]
    release_date: String,
}

#[derive(Debug, Serialize)]
struct CachedSdeMetadata {
    build_number: u64,
    release_date: String,
}

#[derive(Debug, Deserialize)]
struct RawSolarSystem {
    #[serde(rename = "_key", alias = "solarSystemID")]
    solar_system_id: i64,
    name: RawLocalizedName,
    #[serde(rename = "securityStatus")]
    security_status: f64,
}

#[derive(Debug, Deserialize)]
struct RawLocalizedName {
    en: String,
}

#[derive(Debug, Deserialize)]
struct RawStargate {
    #[serde(rename = "_key", alias = "stargateID")]
    stargate_id: i64,
    #[serde(rename = "solarSystemID")]
    solar_system_id: i64,
    destination: Option<RawStargateDestination>,
}

#[derive(Debug, Deserialize)]
struct RawStargateDestination {
    #[serde(rename = "solarSystemID")]
    solar_system_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_graph() -> RouteGraph {
        RouteGraph::from_cache(CachedRouteGraph {
            build_number: 1,
            release_date: "2026-05-19T00:00:00Z".to_string(),
            systems: vec![
                system(1, "Alpha"),
                system(2, "Bravo"),
                system(3, "Charlie"),
                system(4, "Delta"),
                system(5, "Echo"),
            ],
            edges: vec![edge(1, 2), edge(2, 3), edge(1, 4), edge(4, 3), edge(3, 5)],
        })
    }

    fn star_graph() -> RouteGraph {
        RouteGraph::from_cache(CachedRouteGraph {
            build_number: 1,
            release_date: "2026-05-19T00:00:00Z".to_string(),
            systems: vec![
                system(1, "Origin"),
                system(2, "Bravo"),
                system(3, "Charlie"),
                system(4, "Delta"),
            ],
            edges: vec![edge(1, 2), edge(1, 3), edge(1, 4)],
        })
    }

    fn system(id: i64, name: &str) -> SolarSystem {
        SolarSystem {
            id,
            name: name.to_string(),
            security_status: 0.5,
        }
    }

    fn edge(from: i64, to: i64) -> RouteEdge {
        RouteEdge { from, to }
    }

    fn destination(system_id: i64, system_name: &str, import_index: usize) -> RouteDestination {
        RouteDestination {
            system_id,
            system_name: system_name.to_string(),
            import_index,
        }
    }

    #[test]
    fn resolves_system_names_case_insensitively() {
        let graph = test_graph();

        let system = graph
            .resolve_system(" alpha ")
            .expect("system should resolve");

        assert_eq!(system.id, 1);
        assert_eq!(system.name, "Alpha");
    }

    #[test]
    fn dijkstra_finds_shortest_path_distance() {
        let graph = test_graph();

        assert_eq!(graph.distance(1, 5), Some(3));
        assert_eq!(graph.distance(1, 1), Some(0));
        assert_eq!(graph.distance(1, 999), None);
    }

    #[test]
    fn optimizer_uses_shortest_open_route() {
        let graph = test_graph();
        let destinations = vec![destination(5, "Echo", 0), destination(4, "Delta", 1)];

        let route = graph
            .optimize_open_route(1, &destinations)
            .expect("route should optimize");

        assert_eq!(
            route
                .destinations
                .iter()
                .map(|destination| destination.system_id)
                .collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert_eq!(route.total_jumps, 3);
    }

    #[test]
    fn optimizer_keeps_stable_order_for_equal_routes() {
        let graph = test_graph();
        let destinations = vec![destination(2, "Bravo", 0), destination(4, "Delta", 1)];

        let route = graph
            .optimize_open_route(1, &destinations)
            .expect("route should optimize");

        assert_eq!(
            route
                .destinations
                .iter()
                .map(|destination| destination.system_id)
                .collect::<Vec<_>>(),
            vec![2, 4]
        );
    }

    #[test]
    fn optimizer_keeps_import_order_for_three_way_tie() {
        let graph = star_graph();
        let destinations = vec![
            destination(2, "Bravo", 0),
            destination(3, "Charlie", 1),
            destination(4, "Delta", 2),
        ];

        let route = graph
            .optimize_open_route(1, &destinations)
            .expect("route should optimize");

        assert_eq!(
            route
                .destinations
                .iter()
                .map(|destination| destination.system_id)
                .collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        assert_eq!(route.total_jumps, 5);
    }

    #[test]
    fn optimizer_rejects_too_many_destinations() {
        let graph = test_graph();
        let destinations: Vec<RouteDestination> = (0..=MAX_OPTIMIZED_ROUTE_STOPS)
            .map(|index| destination(2, "Bravo", index))
            .collect();

        let err = graph.optimize_open_route(1, &destinations).unwrap_err();

        assert!(err.to_string().contains("or fewer systems"));
    }

    #[test]
    fn parses_solar_system_jsonl_fixture() {
        let data = r#"{"_key":30000142,"name":{"en":"Jita"},"securityStatus":0.945913}
{"_key":30000144,"name":{"en":"Perimeter"},"securityStatus":0.931604}
"#;

        let systems =
            parse_solar_system_reader(BufReader::new(Cursor::new(data))).expect("valid fixture");

        assert_eq!(systems.len(), 2);
        assert_eq!(systems[0].id, 30000142);
        assert_eq!(systems[0].name, "Jita");
    }

    #[test]
    fn parses_stargate_jsonl_fixture() {
        let data = r#"{"_key":50001248,"solarSystemID":30000142,"destination":{"solarSystemID":30000140,"stargateID":50000802}}
{"_key":50001249,"solarSystemID":30000142}
"#;

        let edges =
            parse_stargate_reader(BufReader::new(Cursor::new(data))).expect("valid fixture");

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from, 30000142);
        assert_eq!(edges[0].to, 30000140);
    }
}
