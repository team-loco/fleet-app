const MILLIS_PER_CORE: u32 = 1000;
const KIB: u64 = 1024;
const MIB: u64 = KIB * KIB;
const GIB: u64 = MIB * KIB;
const BYTES_PER_MIB: u64 = MIB;
const MILLIS_PER_CORE_F32: f32 = 1000.;
const GIB_F32: f32 = 1024. * 1024. * 1024.;

pub const CPU_UNIT: &str = "cores";
pub const MEMORY_UNIT: &str = "GiB";
pub const NOT_MEASURED: &str = "—";
pub const WAITING_FOR_METRICS: &str = "Waiting for metrics";

pub fn fixed(value: f32) -> String {
    format!("{value:.1}")
}

pub fn cpu_millis(millis: u32) -> String {
    if millis < MILLIS_PER_CORE {
        return format!("{millis}m");
    }
    let cores = f64::from(millis) / f64::from(MILLIS_PER_CORE);
    format!("{cores:.1} {CPU_UNIT}")
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn cpu_cores(cores: f32) -> String {
    let millis = (cores.max(0.) * MILLIS_PER_CORE_F32).round() as u32;
    cpu_millis(millis)
}

#[allow(clippy::cast_precision_loss)]
pub fn memory_bytes(bytes: u64) -> String {
    if bytes >= GIB {
        format!("{:.1} {MEMORY_UNIT}", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{} MiB", bytes / MIB)
    } else {
        format!("{} KiB", bytes / KIB)
    }
}

pub fn memory_mib(mib: u32) -> String {
    memory_bytes(u64::from(mib) * BYTES_PER_MIB)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn memory_gib(gib: f32) -> String {
    memory_bytes((gib.max(0.) * GIB_F32) as u64)
}

pub fn times(count: u32) -> String {
    match count {
        1 => "once".into(),
        _ => format!("{count} times"),
    }
}

pub fn amount(used: f32, total: f32, unit: &str) -> String {
    format!("{} / {} {unit}", fixed(used), fixed(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_reads_in_millicores_below_a_core_and_cores_above() {
        assert_eq!(cpu_millis(4), "4m");
        assert_eq!(cpu_millis(212), "212m");
        assert_eq!(cpu_millis(1000), "1.0 cores");
        assert_eq!(cpu_millis(2500), "2.5 cores");
        assert_eq!(cpu_cores(0.026), "26m");
        assert_eq!(cpu_cores(0.), "0m");
        assert_eq!(cpu_cores(1.25), "1.2 cores");
    }

    #[test]
    fn memory_spells_its_binary_units() {
        assert_eq!(memory_bytes(512 * KIB), "512 KiB");
        assert_eq!(memory_bytes(38 * MIB), "38 MiB");
        assert_eq!(memory_bytes(301 * MIB + 5), "301 MiB");
        assert_eq!(memory_bytes(GIB), "1.0 GiB");
        assert_eq!(memory_bytes(GIB + GIB / 2), "1.5 GiB");
        assert_eq!(memory_mib(768), "768 MiB");
        assert_eq!(memory_mib(2048), "2.0 GiB");
        assert_eq!(memory_gib(0.25), "256 MiB");
    }

    #[test]
    fn repeat_counts_read_as_words() {
        assert_eq!(times(1), "once");
        assert_eq!(times(18), "18 times");
    }

    #[test]
    fn fixed_keeps_one_decimal() {
        assert_eq!(fixed(61.), "61.0");
        assert_eq!(fixed(0.), "0.0");
        assert_eq!(fixed(22.46), "22.5");
    }

    #[test]
    fn amounts_keep_one_decimal_on_both_sides() {
        assert_eq!(amount(1., 7.8, MEMORY_UNIT), "1.0 / 7.8 GiB");
        assert_eq!(amount(68.4, 112., CPU_UNIT), "68.4 / 112.0 cores");
        assert_eq!(amount(0., 0., MEMORY_UNIT), "0.0 / 0.0 GiB");
    }
}
