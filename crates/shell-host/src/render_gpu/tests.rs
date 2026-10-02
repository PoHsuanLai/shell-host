use std::fs;
use std::path::Path;

use super::*;

/// A sysfs tree with one character device whose PCI function has the given attribute files.
fn tree(root: &Path, number: &str, files: &[(&str, &str)]) {
    let dir = root.join("dev/char").join(number).join("device");
    fs::create_dir_all(&dir).expect("fake sysfs directory");
    for (name, text) in files {
        fs::write(dir.join(name), text).expect("fake sysfs file");
    }
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "shell-host-render-gpu-{name}-{}",
        std::process::id()
    ));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(&root).expect("scratch root");
    root
}

fn render_node(minor: u32) -> DeviceNumber {
    DeviceNumber { major: 226, minor }
}

#[test]
fn a_device_maps_to_the_pci_function_it_sits_on() {
    let root = scratch("table");
    tree(
        &root,
        "226:128",
        &[("vendor", "0x10de\n"), ("device", "0x2c05\n")],
    );
    tree(
        &root,
        "226:129",
        &[("vendor", "0x1002\n"), ("device", "0x13c0\n")],
    );
    tree(&root, "226:130", &[("vendor", "0x1002\n")]);
    tree(
        &root,
        "226:131",
        &[("vendor", "nonsense\n"), ("device", "0x1\n")],
    );
    tree(
        &root,
        "226:132",
        &[("vendor", "0x8086\n"), ("device", "0x0\n")],
    );
    let ids = |vendor, device| {
        Some(PciIds {
            vendor: PciVendor(vendor),
            device: PciDevice(device),
        })
    };
    let cases: &[(&str, DeviceNumber, Option<PciIds>)] = &[
        ("nvidia discrete", render_node(128), ids(0x10de, 0x2c05)),
        ("amd integrated", render_node(129), ids(0x1002, 0x13c0)),
        ("no device file", render_node(130), None),
        ("unparsable vendor", render_node(131), None),
        ("device id zero is an id", render_node(132), ids(0x8086, 0)),
        ("not in the tree", render_node(200), None),
        (
            "another major",
            DeviceNumber {
                major: 3,
                minor: 128,
            },
            None,
        ),
    ];
    for (name, number, want) in cases {
        assert_eq!(pci_ids(&root, *number), *want, "{name}");
    }
    drop(fs::remove_dir_all(&root));
}

#[test]
fn a_missing_sysfs_root_has_no_ids() {
    assert_eq!(
        pci_ids(Path::new("/nonexistent/sysfs"), render_node(128)),
        None
    );
}

#[test]
fn device_numbers_decode_like_glibc() {
    // 0xE280 is 226:128, /dev/dri/renderD128 (wayland-info's `main device: 0xE280`).
    let cases: &[(u64, Option<(u32, u32)>)] = &[
        (0xE280, Some((226, 128))),
        (0xE281, Some((226, 129))),
        // glibc's makedev(511, 300): a minor above 255 uses bits 20 and up.
        (0x0011_ff2c, Some((511, 300))),
    ];
    for (dev, want) in cases {
        let got = DeviceNumber::from_wire(&dev.to_ne_bytes()).map(|d| (d.major, d.minor));
        assert_eq!(got, *want, "{dev:#x}");
    }
    assert_eq!(DeviceNumber::from_wire(&[0; 4]), None);
}

#[test]
fn only_auto_follows_the_compositor() {
    let compositor = Some(PciIds {
        vendor: PciVendor(0x1002),
        device: PciDevice(0x13c0),
    });
    let followed = AdapterPref::Device {
        vendor: PciVendor(0x1002),
        device: PciDevice(0x13c0),
    };
    let cases: &[(&str, AdapterPref, Option<PciIds>, AdapterPref)] = &[
        ("auto follows", AdapterPref::Auto, compositor, followed),
        (
            "auto, compositor unknown",
            AdapterPref::Auto,
            None,
            AdapterPref::Auto,
        ),
        (
            "discrete stands",
            AdapterPref::Discrete,
            compositor,
            AdapterPref::Discrete,
        ),
        (
            "named stands",
            AdapterPref::Named("NVIDIA".into()),
            compositor,
            AdapterPref::Named("NVIDIA".into()),
        ),
    ];
    for (name, configured, ids, want) in cases {
        assert_eq!(follow_compositor(configured, *ids), *want, "{name}");
    }
}

#[test]
fn the_environment_name_beats_the_followed_device() {
    let followed = follow_compositor(
        &AdapterPref::Auto,
        Some(PciIds {
            vendor: PciVendor(1),
            device: PciDevice(2),
        }),
    );
    assert_eq!(
        followed.with_env(Some("NVIDIA")),
        AdapterPref::Named("NVIDIA".into())
    );
}
