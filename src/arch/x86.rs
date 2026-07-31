use std::hash::{DefaultHasher, Hasher};

use raw_cpuid::CpuId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum X86Level {
    X86_64,
    V2,
    V3,
    V4,
}

pub fn detect_x86_level() -> X86Level {
    let cpuid = CpuId::new();

    let Some(fi) = cpuid.get_feature_info() else {
        return X86Level::X86_64;
    };

    let Some(epfi) = cpuid.get_extended_processor_and_feature_identifiers() else {
        return X86Level::X86_64;
    };

    let has_v2 = fi.has_sse3()
        && fi.has_ssse3()
        && fi.has_sse41()
        && fi.has_sse42()
        && fi.has_popcnt()
        && fi.has_cmpxchg16b()
        && epfi.has_lahf_sahf();

    if !has_v2 {
        return X86Level::X86_64;
    }

    let Some(efi) = cpuid.get_extended_feature_info() else {
        return X86Level::V2;
    };

    let xcr0 = if fi.has_xsave() && fi.has_oxsave() {
        Some(unsafe { std::arch::x86_64::_xgetbv(0) })
    } else {
        None
    };
    let has_avx_state = xcr0.is_some_and(|value| value & 0b110 == 0b110);
    let has_v3 = fi.has_avx()
        && fi.has_fma()
        && fi.has_f16c()
        && fi.has_movbe()
        && has_avx_state
        && efi.has_avx2()
        && efi.has_bmi1()
        && efi.has_bmi2()
        && epfi.has_lzcnt();

    if !has_v3 {
        return X86Level::V2;
    }

    let has_avx512_state = xcr0.is_some_and(|value| value & 0b1110_0110 == 0b1110_0110);
    let has_v4 = has_avx512_state
        && efi.has_avx512f()
        && efi.has_avx512bw()
        && efi.has_avx512cd()
        && efi.has_avx512dq()
        && efi.has_avx512vl();

    if has_v4 { X86Level::V4 } else { X86Level::V3 }
}

pub fn native_hasher() -> Option<u64> {
    let cpuid = CpuId::new();

    let vendor = cpuid.get_vendor_info().map(|v| v.as_str().trim().to_string())?;
    let brand = cpuid
        .get_processor_brand_string()
        .map(|b| b.as_str().trim().to_string())?;

    let mut parts = Vec::new();

    parts.push(format!("target_arch={}", std::env::consts::ARCH));
    parts.push(format!("target_os={}", std::env::consts::OS));
    parts.push(format!("target_family={}", std::env::consts::FAMILY));
    parts.push(format!("target_pointer_width={}", std::mem::size_of::<usize>() * 8));
    parts.push(format!(
        "target_endian={}",
        if cfg!(target_endian = "little") {
            "little"
        } else {
            "big"
        }
    ));

    parts.push(format!("cpu_vendor={vendor}"));
    parts.push(format!("cpu_brand={brand}"));

    if let Some(fi) = cpuid.get_feature_info() {
        parts.push(format!("family={}", fi.family_id()));
        parts.push(format!("model={}", fi.model_id()));
        parts.push(format!("stepping={}", fi.stepping_id()));
        parts.push(format!("base_family={}", fi.base_family_id()));
        parts.push(format!("base_model={}", fi.base_model_id()));
        parts.push(format!("extended_family={}", fi.extended_family_id()));
        parts.push(format!("extended_model={}", fi.extended_model_id()));

        push_feature(&mut parts, "sse", fi.has_sse());
        push_feature(&mut parts, "sse2", fi.has_sse2());
        push_feature(&mut parts, "sse3", fi.has_sse3());
        push_feature(&mut parts, "ssse3", fi.has_ssse3());
        push_feature(&mut parts, "sse4.1", fi.has_sse41());
        push_feature(&mut parts, "sse4.2", fi.has_sse42());
        push_feature(&mut parts, "popcnt", fi.has_popcnt());
        push_feature(&mut parts, "aes", fi.has_aesni());
        push_feature(&mut parts, "pclmulqdq", fi.has_pclmulqdq());
        push_feature(&mut parts, "rdrand", fi.has_rdrand());
        push_feature(&mut parts, "f16c", fi.has_f16c());
        push_feature(&mut parts, "fma", fi.has_fma());
        push_feature(&mut parts, "movbe", fi.has_movbe());
        push_feature(&mut parts, "xsave", fi.has_xsave());
        push_feature(&mut parts, "osxsave", fi.has_oxsave());
        push_feature(&mut parts, "avx", fi.has_avx());
        push_feature(&mut parts, "cmpxchg16b", fi.has_cmpxchg16b());

        if fi.has_xsave() && fi.has_oxsave() {
            let xcr0 = unsafe { std::arch::x86_64::_xgetbv(0) };
            parts.push(format!("xcr0={xcr0}"));
        }
    }

    if let Some(epfi) = cpuid.get_extended_processor_and_feature_identifiers() {
        push_feature(&mut parts, "lahf_sahf", epfi.has_lahf_sahf());
        push_feature(&mut parts, "lzcnt", epfi.has_lzcnt());
    }

    if let Some(efi) = cpuid.get_extended_feature_info() {
        push_feature(&mut parts, "avx2", efi.has_avx2());
        push_feature(&mut parts, "bmi1", efi.has_bmi1());
        push_feature(&mut parts, "bmi2", efi.has_bmi2());
        push_feature(&mut parts, "avx512f", efi.has_avx512f());
        push_feature(&mut parts, "avx512bw", efi.has_avx512bw());
        push_feature(&mut parts, "avx512cd", efi.has_avx512cd());
        push_feature(&mut parts, "avx512dq", efi.has_avx512dq());
        push_feature(&mut parts, "avx512vl", efi.has_avx512vl());
    }

    parts.sort();
    let bytes = parts.join("\n");

    let mut hasher = DefaultHasher::new();

    hasher.write(bytes.as_bytes());
    let hash = hasher.finish();
    Some(hash)
}

fn push_feature(parts: &mut Vec<String>, name: &'static str, enabled: bool) {
    if enabled {
        parts.push(format!("feature={name}"));
    }
}
