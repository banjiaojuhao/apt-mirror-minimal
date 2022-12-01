use bytes::{Buf, Bytes};
use std::{collections::HashMap, fs, io::Read, path::Path, process::exit};

use reqwest;

const USER_AGENT: &str = "Debian APT-HTTP/1.3 (2.0.9) non-interactive";

#[derive(Default, Debug)]
struct Package {
    name: String,
    architecture: String,
    version: String,
    depends: String,
    suggests: String,
    filename: String,
    size: u64,
    md5sum: String,
    sha1: String,
    sha256: String,
}

fn main() {
    env_logger::init();
    let cache_root = "/tmp/apt-mirror-minimal";
    let cache_root = Path::new(cache_root);
    let archive_root = "https://mirrors.bfsu.edu.cn/ubuntu";
    let os_id = "ubuntu";
    let distribution = "focal";
    let component_list = "main restricted";
    let arch_list = "amd64 i386";
    let extension_list = " .gz .xz";

    let archive_root_cache = cache_root.join(os_id);

    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .unwrap();

    let dist_base = format!("{}/dists/{}", archive_root, distribution);
    let dist_base_cache = archive_root_cache.join("dists").join(distribution);
    fs::create_dir_all(&dist_base_cache).unwrap();

    // download InRelease, Release, Release.gpg
    let mut release_text: Option<Bytes> = None;
    let resp = client
        .get(format!("{}/InRelease", dist_base))
        .send()
        .unwrap();
    if resp.status().is_success() {
        fs::write(dist_base_cache.join("InRelease"), resp.bytes().unwrap()).unwrap();
        log::info!("downloaded InRelease");
    } else {
        log::error!("failed to download InRelease");
    }
    let resp = client.get(format!("{}/Release", dist_base)).send().unwrap();
    if resp.status().is_success() {
        let resp_content = resp.bytes().unwrap();
        fs::write(dist_base_cache.join("Release"), &resp_content).unwrap();
        release_text = Some(resp_content.to_owned());
        log::info!("downloaded Release");
    } else {
        log::error!("failed to download Release");
    }
    let resp = client
        .get(format!("{}/Release.gpg", dist_base))
        .send()
        .unwrap();
    if resp.status().is_success() {
        fs::write(dist_base_cache.join("Release.gpg"), resp.bytes().unwrap()).unwrap();
        log::info!("downloaded Release.gpg");
    } else {
        log::error!("failed to download Release.gpg");
    }

    if release_text.is_none() {
        log::error!("no release file, exit");
        exit(1);
    }
    // TODO: verify Release file

    // parse release file
    let release_text = String::from_utf8(release_text.unwrap().to_vec()).unwrap();
    let release_lines = release_text.lines();
    let mut path2md5 = HashMap::new();
    let mut start_md5 = false;
    for line in release_lines {
        if line.starts_with("MD5Sum") {
            start_md5 = true;
            continue;
        }
        if start_md5 {
            if line.starts_with(" ") {
                let mut cols = line.split_whitespace();
                let md5sum = cols.next().unwrap().to_owned();
                let _size = cols.next().unwrap().to_owned();
                let path = cols.next().unwrap().to_owned();
                path2md5.insert(path, md5sum);
            } else {
                start_md5 = false;
            }
        }
    }

    for arch in arch_list.split(" ") {
        let mut package_info = HashMap::new();
        for component in component_list.split(" ") {
            // download Packages{,.gz,.xz}
            let mut parsed = false;
            for extention in extension_list.split(" ") {
                let package_file = format!("{}/binary-{}/Packages{}", component, arch, extention);
                if path2md5.contains_key(&package_file) {
                    let resp = client
                        .get(format!("{}/{}", dist_base, package_file))
                        .send()
                        .unwrap();
                    if resp.status().is_success() {
                        let resp_content = resp.bytes().unwrap();
                        let save_path = dist_base_cache.join(&package_file);
                        fs::create_dir_all(save_path.parent().unwrap()).unwrap();
                        fs::write(save_path, &resp_content).unwrap();
                        log::info!("downloaded {}", &package_file);
                        // parse Package file
                        if !parsed {
                            parsed = true;
                            let package_text = match extention {
                                "" => String::from_utf8(resp_content.to_vec()).unwrap(),
                                ".gz" => {
                                    let mut x = String::new();
                                    let mut d = flate2::read::GzDecoder::new(resp_content.reader());
                                    d.read_to_string(&mut x).unwrap();
                                    x
                                }
                                ".xz" => {
                                    let mut x = String::new();
                                    let mut d = xz2::read::XzDecoder::new(resp_content.reader());
                                    d.read_to_string(&mut x).unwrap();
                                    x
                                }
                                _ => String::new(),
                            };
                            let mut package: Package = Default::default();
                            for line in package_text.lines() {
                                if line.is_empty() {
                                    package_info.insert(package.name.to_owned(), package);
                                    package = Default::default();
                                }
                                let mut line = line.split(": ");
                                match line.next().unwrap() {
                                    "Package" => package.name = line.next().unwrap().to_owned(),
                                    "Architecture" => {
                                        package.architecture = line.next().unwrap().to_owned()
                                    }
                                    "Version" => package.version = line.next().unwrap().to_owned(),
                                    "Depends" => package.depends = line.next().unwrap().to_owned(),
                                    "Suggests" => {
                                        package.suggests = line.next().unwrap().to_owned()
                                    }
                                    "Filename" => {
                                        package.filename = line.next().unwrap().to_owned()
                                    }
                                    "Size" => package.size = line.next().unwrap().parse().unwrap(),
                                    "MD5sum" => package.md5sum = line.next().unwrap().to_owned(),
                                    "SHA1" => package.sha1 = line.next().unwrap().to_owned(),
                                    "SHA256" => package.sha256 = line.next().unwrap().to_owned(),
                                    _ => {}
                                }
                            }
                        }
                    } else if resp.status().as_u16() == 404 {
                        log::error!("file {} not found", package_file);
                    } else {
                        log::error!(
                            "failed to download {}. {}",
                            package_file,
                            resp.text().unwrap()
                        );
                    }
                }
            }
        }
        log::info!("{:?}", package_info.get("curl").unwrap());
    }
}
