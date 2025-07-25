//! Interrupt handling

use procmacros::ram;
use xtensa_lx::interrupt;
#[cfg(esp32)]
pub(crate) use xtensa_lx::interrupt::free;
#[cfg(feature = "rt")]
use xtensa_lx_rt::exception::Context;

pub use self::vectored::*;
use super::InterruptStatus;
use crate::{pac, peripherals::Interrupt, system::Cpu};

/// Interrupt Error
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    /// The given interrupt is not a valid interrupt
    InvalidInterrupt,
    /// The CPU interrupt is a reserved interrupt
    CpuInterruptReserved,
}

/// Enumeration of available CPU interrupts
///
/// It's possible to create one handler per priority level. (e.g
/// `level1_interrupt`)
#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[repr(u32)]
pub enum CpuInterrupt {
    /// Level-triggered interrupt with priority 1.
    Interrupt0LevelPriority1 = 0,
    /// Level-triggered interrupt with priority 1.
    Interrupt1LevelPriority1,
    /// Level-triggered interrupt with priority 1.
    Interrupt2LevelPriority1,
    /// Level-triggered interrupt with priority 1.
    Interrupt3LevelPriority1,
    /// Level-triggered interrupt with priority 1.
    Interrupt4LevelPriority1,
    /// Level-triggered interrupt with priority 1.
    Interrupt5LevelPriority1,
    /// Timer 0 interrupt with priority 1.
    Interrupt6Timer0Priority1,
    /// Software-triggered interrupt with priority 1.
    Interrupt7SoftwarePriority1,
    /// Level-triggered interrupt with priority 1.
    Interrupt8LevelPriority1,
    /// Level-triggered interrupt with priority 1.
    Interrupt9LevelPriority1,
    /// Edge-triggered interrupt with priority 1.
    Interrupt10EdgePriority1,
    /// Profiling-related interrupt with priority 3.
    Interrupt11ProfilingPriority3,
    /// Level-triggered interrupt with priority 1.
    Interrupt12LevelPriority1,
    /// Level-triggered interrupt with priority 1.
    Interrupt13LevelPriority1,
    /// Non-maskable interrupt (NMI) with priority 7.
    Interrupt14NmiPriority7,
    /// Timer 1 interrupt with priority 3.
    Interrupt15Timer1Priority3,
    /// Timer 2 interrupt with priority 5.
    Interrupt16Timer2Priority5,
    /// Level-triggered interrupt with priority 1.
    Interrupt17LevelPriority1,
    /// Level-triggered interrupt with priority 1.
    Interrupt18LevelPriority1,
    /// Level-triggered interrupt with priority 2.
    Interrupt19LevelPriority2,
    /// Level-triggered interrupt with priority 2.
    Interrupt20LevelPriority2,
    /// Level-triggered interrupt with priority 2.
    Interrupt21LevelPriority2,
    /// Edge-triggered interrupt with priority 3.
    Interrupt22EdgePriority3,
    /// Level-triggered interrupt with priority 3.
    Interrupt23LevelPriority3,
    /// Level-triggered interrupt with priority 4.
    Interrupt24LevelPriority4,
    /// Level-triggered interrupt with priority 4.
    Interrupt25LevelPriority4,
    /// Level-triggered interrupt with priority 5.
    Interrupt26LevelPriority5,
    /// Level-triggered interrupt with priority 3.
    Interrupt27LevelPriority3,
    /// Edge-triggered interrupt with priority 4.
    Interrupt28EdgePriority4,
    /// Software-triggered interrupt with priority 3.
    Interrupt29SoftwarePriority3,
    /// Edge-triggered interrupt with priority 4.
    Interrupt30EdgePriority4,
    /// Edge-triggered interrupt with priority 5.
    Interrupt31EdgePriority5,
}

impl CpuInterrupt {
    fn from_u32(n: u32) -> Option<Self> {
        if n < 32 {
            Some(unsafe { core::mem::transmute::<u32, Self>(n) })
        } else {
            None
        }
    }

    fn is_internal(self) -> bool {
        matches!(
            self,
            Self::Interrupt6Timer0Priority1
                | Self::Interrupt7SoftwarePriority1
                | Self::Interrupt11ProfilingPriority3
                | Self::Interrupt15Timer1Priority3
                | Self::Interrupt16Timer2Priority5
                | Self::Interrupt29SoftwarePriority3
        )
    }

    fn is_peripheral(self) -> bool {
        !self.is_internal()
    }
}

/// The interrupts reserved by the HAL
#[cfg_attr(place_switch_tables_in_ram, unsafe(link_section = ".rwtext"))]
pub static RESERVED_INTERRUPTS: &[u32] = &[
    CpuInterrupt::Interrupt1LevelPriority1 as _,
    CpuInterrupt::Interrupt19LevelPriority2 as _,
    CpuInterrupt::Interrupt23LevelPriority3 as _,
    CpuInterrupt::Interrupt10EdgePriority1 as _,
    CpuInterrupt::Interrupt22EdgePriority3 as _,
];

pub(crate) fn setup_interrupts() {
    // disable all known interrupts
    // at least after the 2nd stage bootloader there are some interrupts enabled
    // (e.g. UART)
    for peripheral_interrupt in 0..255 {
        Interrupt::try_from(peripheral_interrupt)
            .map(|intr| {
                #[cfg(multi_core)]
                disable(Cpu::AppCpu, intr);
                disable(Cpu::ProCpu, intr);
            })
            .ok();
    }
}

/// Enable an interrupt by directly binding it to a available CPU interrupt
///
/// Unless you are sure, you most likely want to use [`enable`] instead.
///
/// Trying using a reserved interrupt from [`RESERVED_INTERRUPTS`] will return
/// an error.
pub fn enable_direct(interrupt: Interrupt, cpu_interrupt: CpuInterrupt) -> Result<(), Error> {
    if RESERVED_INTERRUPTS.contains(&(cpu_interrupt as _)) {
        return Err(Error::CpuInterruptReserved);
    }
    unsafe {
        map(Cpu::current(), interrupt, cpu_interrupt);

        xtensa_lx::interrupt::enable_mask(
            xtensa_lx::interrupt::get_mask() | (1 << cpu_interrupt as u32),
        );
    }
    Ok(())
}

/// Assign a peripheral interrupt to an CPU interrupt
///
/// Note: this only maps the interrupt to the CPU interrupt. The CPU interrupt
/// still needs to be enabled afterwards
///
/// # Safety
///
/// Do not use CPU interrupts in the [`RESERVED_INTERRUPTS`].
pub unsafe fn map(cpu: Cpu, interrupt: Interrupt, which: CpuInterrupt) {
    let interrupt_number = interrupt as usize;
    let cpu_interrupt_number = which as u32;
    match cpu {
        Cpu::ProCpu => unsafe {
            (*core0_interrupt_peripheral())
                .core_0_intr_map(interrupt_number)
                .write(|w| w.bits(cpu_interrupt_number));
        },
        #[cfg(multi_core)]
        Cpu::AppCpu => unsafe {
            (*core1_interrupt_peripheral())
                .core_1_intr_map(interrupt_number)
                .write(|w| w.bits(cpu_interrupt_number));
        },
    }
}

/// Get cpu interrupt assigned to peripheral interrupt
pub(crate) fn bound_cpu_interrupt_for(cpu: Cpu, interrupt: Interrupt) -> Option<CpuInterrupt> {
    let cpu_intr = match cpu {
        Cpu::ProCpu => unsafe {
            (*core0_interrupt_peripheral())
                .core_0_intr_map(interrupt as usize)
                .read()
                .bits()
        },
        #[cfg(multi_core)]
        Cpu::AppCpu => unsafe {
            (*core1_interrupt_peripheral())
                .core_1_intr_map(interrupt as usize)
                .read()
                .bits()
        },
    };
    let cpu_intr = CpuInterrupt::from_u32(cpu_intr)?;

    if cpu_intr.is_peripheral() {
        Some(cpu_intr)
    } else {
        None
    }
}

/// Disable the given peripheral interrupt
pub fn disable(core: Cpu, interrupt: Interrupt) {
    unsafe { map(core, interrupt, CpuInterrupt::Interrupt16Timer2Priority5) }
}

/// Clear the given CPU interrupt
pub fn clear(_core: Cpu, which: CpuInterrupt) {
    unsafe {
        xtensa_lx::interrupt::clear(1 << which as u32);
    }
}

/// Get status of peripheral interrupts
#[cfg(interrupts_status_registers = "3")]
pub fn status(core: Cpu) -> InterruptStatus {
    unsafe {
        match core {
            Cpu::ProCpu => InterruptStatus::from(
                (*core0_interrupt_peripheral())
                    .core_0_intr_status(0)
                    .read()
                    .bits(),
                (*core0_interrupt_peripheral())
                    .core_0_intr_status(1)
                    .read()
                    .bits(),
                (*core0_interrupt_peripheral())
                    .core_0_intr_status(2)
                    .read()
                    .bits(),
            ),
            #[cfg(multi_core)]
            Cpu::AppCpu => InterruptStatus::from(
                (*core1_interrupt_peripheral())
                    .core_1_intr_status(0)
                    .read()
                    .bits(),
                (*core1_interrupt_peripheral())
                    .core_1_intr_status(1)
                    .read()
                    .bits(),
                (*core1_interrupt_peripheral())
                    .core_1_intr_status(2)
                    .read()
                    .bits(),
            ),
        }
    }
}

/// Get status of peripheral interrupts
#[cfg(interrupts_status_registers = "4")]
pub fn status(core: Cpu) -> InterruptStatus {
    unsafe {
        match core {
            Cpu::ProCpu => InterruptStatus::from(
                (*core0_interrupt_peripheral())
                    .core_0_intr_status(0)
                    .read()
                    .bits(),
                (*core0_interrupt_peripheral())
                    .core_0_intr_status(1)
                    .read()
                    .bits(),
                (*core0_interrupt_peripheral())
                    .core_0_intr_status(2)
                    .read()
                    .bits(),
                (*core0_interrupt_peripheral())
                    .core_0_intr_status(3)
                    .read()
                    .bits(),
            ),
            #[cfg(multi_core)]
            Cpu::AppCpu => InterruptStatus::from(
                (*core1_interrupt_peripheral())
                    .core_1_intr_status(0)
                    .read()
                    .bits(),
                (*core1_interrupt_peripheral())
                    .core_1_intr_status(1)
                    .read()
                    .bits(),
                (*core1_interrupt_peripheral())
                    .core_1_intr_status(2)
                    .read()
                    .bits(),
                (*core1_interrupt_peripheral())
                    .core_1_intr_status(3)
                    .read()
                    .bits(),
            ),
        }
    }
}

#[cfg(esp32)]
unsafe fn core0_interrupt_peripheral() -> *const crate::pac::dport::RegisterBlock {
    pac::DPORT::PTR
}

#[cfg(esp32)]
unsafe fn core1_interrupt_peripheral() -> *const crate::pac::dport::RegisterBlock {
    pac::DPORT::PTR
}

#[cfg(any(esp32s2, esp32s3))]
unsafe fn core0_interrupt_peripheral() -> *const crate::pac::interrupt_core0::RegisterBlock {
    pac::INTERRUPT_CORE0::PTR
}

#[cfg(esp32s3)]
unsafe fn core1_interrupt_peripheral() -> *const crate::pac::interrupt_core1::RegisterBlock {
    pac::INTERRUPT_CORE1::PTR
}

/// Get the current run level (the level below which interrupts are masked).
pub fn current_runlevel() -> Priority {
    let ps: u32;
    unsafe { core::arch::asm!("rsr.ps {0}", out(reg) ps) };

    let prev_interrupt_priority = ps as u8 & 0x0F;

    unwrap!(Priority::try_from(prev_interrupt_priority))
}

/// Changes the current run level (the level below which interrupts are
/// masked), and returns the previous run level.
///
/// # Safety
///
/// This function must only be used to raise the runlevel and to restore it
/// to a previous value. It must not be used to arbitrarily lower the
/// runlevel.
pub(crate) unsafe fn change_current_runlevel(level: Priority) -> Priority {
    let token: u32;
    unsafe {
        match level {
            Priority::None => core::arch::asm!("rsil {0}, 0", out(reg) token),
            Priority::Priority1 => core::arch::asm!("rsil {0}, 1", out(reg) token),
            Priority::Priority2 => core::arch::asm!("rsil {0}, 2", out(reg) token),
            Priority::Priority3 => core::arch::asm!("rsil {0}, 3", out(reg) token),
        };
    }

    let prev_interrupt_priority = token as u8 & 0x0F;

    unwrap!(Priority::try_from(prev_interrupt_priority))
}

mod vectored {
    use super::*;

    /// Interrupt priority levels.
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    #[repr(u8)]
    pub enum Priority {
        /// No priority.
        None = 0,
        /// Priority level 1.
        Priority1,
        /// Priority level 2.
        Priority2,
        /// Priority level 3.
        Priority3,
    }

    impl Priority {
        /// Maximum interrupt priority
        pub const fn max() -> Priority {
            Priority::Priority3
        }

        /// Minimum interrupt priority
        pub const fn min() -> Priority {
            Priority::Priority1
        }
    }

    impl TryFrom<u32> for Priority {
        type Error = Error;

        fn try_from(value: u32) -> Result<Self, Self::Error> {
            match value {
                0 => Ok(Priority::None),
                1 => Ok(Priority::Priority1),
                2 => Ok(Priority::Priority2),
                3 => Ok(Priority::Priority3),
                _ => Err(Error::InvalidInterrupt),
            }
        }
    }

    impl TryFrom<u8> for Priority {
        type Error = Error;

        fn try_from(value: u8) -> Result<Self, Self::Error> {
            Self::try_from(value as u32)
        }
    }

    impl CpuInterrupt {
        #[inline]
        fn level(&self) -> Priority {
            match self {
                CpuInterrupt::Interrupt0LevelPriority1
                | CpuInterrupt::Interrupt1LevelPriority1
                | CpuInterrupt::Interrupt2LevelPriority1
                | CpuInterrupt::Interrupt3LevelPriority1
                | CpuInterrupt::Interrupt4LevelPriority1
                | CpuInterrupt::Interrupt5LevelPriority1
                | CpuInterrupt::Interrupt6Timer0Priority1
                | CpuInterrupt::Interrupt7SoftwarePriority1
                | CpuInterrupt::Interrupt8LevelPriority1
                | CpuInterrupt::Interrupt9LevelPriority1
                | CpuInterrupt::Interrupt10EdgePriority1
                | CpuInterrupt::Interrupt12LevelPriority1
                | CpuInterrupt::Interrupt13LevelPriority1
                | CpuInterrupt::Interrupt17LevelPriority1
                | CpuInterrupt::Interrupt18LevelPriority1 => Priority::Priority1,

                CpuInterrupt::Interrupt19LevelPriority2
                | CpuInterrupt::Interrupt20LevelPriority2
                | CpuInterrupt::Interrupt21LevelPriority2 => Priority::Priority2,

                CpuInterrupt::Interrupt11ProfilingPriority3
                | CpuInterrupt::Interrupt15Timer1Priority3
                | CpuInterrupt::Interrupt22EdgePriority3
                | CpuInterrupt::Interrupt27LevelPriority3
                | CpuInterrupt::Interrupt29SoftwarePriority3
                | CpuInterrupt::Interrupt23LevelPriority3 => Priority::Priority3,

                // we direct these to None because we do not support interrupts at this level
                // through Rust
                CpuInterrupt::Interrupt24LevelPriority4
                | CpuInterrupt::Interrupt25LevelPriority4
                | CpuInterrupt::Interrupt28EdgePriority4
                | CpuInterrupt::Interrupt30EdgePriority4
                | CpuInterrupt::Interrupt31EdgePriority5
                | CpuInterrupt::Interrupt16Timer2Priority5
                | CpuInterrupt::Interrupt26LevelPriority5
                | CpuInterrupt::Interrupt14NmiPriority7 => Priority::None,
            }
        }
    }

    /// Get the interrupts configured for the core
    #[inline(always)]
    pub(crate) fn configured_interrupts(
        core: Cpu,
        status: InterruptStatus,
        level: u32,
    ) -> InterruptStatus {
        unsafe {
            let intr_map_base = match core {
                Cpu::ProCpu => (*core0_interrupt_peripheral()).core_0_intr_map(0).as_ptr(),
                #[cfg(multi_core)]
                Cpu::AppCpu => (*core1_interrupt_peripheral()).core_1_intr_map(0).as_ptr(),
            };

            let mut res = InterruptStatus::empty();

            for interrupt_nr in status.iterator() {
                let i = interrupt_nr as isize;
                let cpu_interrupt = intr_map_base.offset(i).read_volatile();
                // safety: cast is safe because of repr(u32)
                let cpu_interrupt = core::mem::transmute::<u32, CpuInterrupt>(cpu_interrupt);
                let int_level = cpu_interrupt.level() as u32;

                if int_level == level {
                    res.set(interrupt_nr);
                }
            }

            res
        }
    }

    /// Enable the given peripheral interrupt
    pub fn enable(interrupt: Interrupt, level: Priority) -> Result<(), Error> {
        enable_on_cpu(Cpu::current(), interrupt, level)
    }

    pub(crate) fn enable_on_cpu(
        cpu: Cpu,
        interrupt: Interrupt,
        level: Priority,
    ) -> Result<(), Error> {
        let cpu_interrupt =
            interrupt_level_to_cpu_interrupt(level, chip_specific::interrupt_is_edge(interrupt))?;

        unsafe {
            map(cpu, interrupt, cpu_interrupt);

            xtensa_lx::interrupt::enable_mask(
                xtensa_lx::interrupt::get_mask() | (1 << cpu_interrupt as u32),
            );
        }
        Ok(())
    }

    /// Binds the given interrupt to the given handler.
    ///
    /// # Safety
    ///
    /// This will replace any previously bound interrupt handler
    pub unsafe fn bind_interrupt(interrupt: Interrupt, handler: unsafe extern "C" fn()) {
        let ptr = unsafe {
            &pac::__INTERRUPTS[interrupt as usize]._handler as *const _
                as *mut unsafe extern "C" fn()
        };
        unsafe {
            ptr.write_volatile(handler);
        }
    }

    /// Returns the currently bound interrupt handler.
    pub fn bound_handler(interrupt: Interrupt) -> Option<unsafe extern "C" fn()> {
        unsafe {
            let addr = pac::__INTERRUPTS[interrupt as usize]._handler;
            if addr as usize == 0 {
                return None;
            }
            Some(addr)
        }
    }

    fn interrupt_level_to_cpu_interrupt(
        level: Priority,
        is_edge: bool,
    ) -> Result<CpuInterrupt, Error> {
        Ok(if is_edge {
            match level {
                Priority::None => return Err(Error::InvalidInterrupt),
                Priority::Priority1 => CpuInterrupt::Interrupt10EdgePriority1,
                Priority::Priority2 => return Err(Error::InvalidInterrupt),
                Priority::Priority3 => CpuInterrupt::Interrupt22EdgePriority3,
            }
        } else {
            match level {
                Priority::None => return Err(Error::InvalidInterrupt),
                Priority::Priority1 => CpuInterrupt::Interrupt1LevelPriority1,
                Priority::Priority2 => CpuInterrupt::Interrupt19LevelPriority2,
                Priority::Priority3 => CpuInterrupt::Interrupt23LevelPriority3,
            }
        })
    }

    // TODO use CpuInterrupt::LevelX.mask() // TODO make it const
    #[cfg_attr(place_switch_tables_in_ram, unsafe(link_section = ".rwtext"))]
    pub(crate) static CPU_INTERRUPT_LEVELS: [u32; 8] = [
        0b_0000_0000_0000_0000_0000_0000_0000_0000, // Dummy level 0
        0b_0000_0000_0000_0110_0011_0111_1111_1111, // Level_1
        0b_0000_0000_0011_1000_0000_0000_0000_0000, // Level 2
        0b_0010_1000_1100_0000_1000_1000_0000_0000, // Level 3
        0b_0101_0011_0000_0000_0000_0000_0000_0000, // Level 4
        0b_1000_0100_0000_0001_0000_0000_0000_0000, // Level 5
        0b_0000_0000_0000_0000_0000_0000_0000_0000, // Level 6
        0b_0000_0000_0000_0000_0100_0000_0000_0000, // Level 7
    ];
    #[cfg_attr(place_switch_tables_in_ram, unsafe(link_section = ".rwtext"))]
    pub(crate) static CPU_INTERRUPT_INTERNAL: u32 = 0b_0010_0000_0000_0001_1000_1000_1100_0000;
    #[cfg_attr(place_switch_tables_in_ram, unsafe(link_section = ".rwtext"))]
    pub(crate) static CPU_INTERRUPT_EDGE: u32 = 0b_0111_0000_0100_0000_0000_1100_1000_0000;

    #[cfg(esp32)]
    pub(crate) mod chip_specific {
        use super::*;
        #[cfg_attr(place_switch_tables_in_ram, unsafe(link_section = ".rwtext"))]
        pub static INTERRUPT_EDGE: InterruptStatus = InterruptStatus::from(
            0b0000_0000_0000_0000_0000_0000_0000_0000,
            0b1111_1100_0000_0000_0000_0000_0000_0000,
            0b0000_0000_0000_0000_0000_0000_0000_0011,
        );
        #[inline]
        pub fn interrupt_is_edge(interrupt: Interrupt) -> bool {
            [
                Interrupt::TG0_T0_EDGE,
                Interrupt::TG0_T1_EDGE,
                Interrupt::TG0_WDT_EDGE,
                Interrupt::TG0_LACT_EDGE,
                Interrupt::TG1_T0_EDGE,
                Interrupt::TG1_T1_EDGE,
                Interrupt::TG1_WDT_EDGE,
                Interrupt::TG1_LACT_EDGE,
            ]
            .contains(&interrupt)
        }
    }

    #[cfg(esp32s2)]
    pub(crate) mod chip_specific {
        use super::*;
        #[cfg_attr(place_switch_tables_in_ram, unsafe(link_section = ".rwtext"))]
        pub static INTERRUPT_EDGE: InterruptStatus = InterruptStatus::from(
            0b0000_0000_0000_0000_0000_0000_0000_0000,
            0b1100_0000_0000_0000_0000_0000_0000_0000,
            0b0000_0000_0000_0000_0000_0011_1011_1111,
        );
        #[inline]
        pub fn interrupt_is_edge(interrupt: Interrupt) -> bool {
            [
                Interrupt::TG0_T0_EDGE,
                Interrupt::TG0_T1_EDGE,
                Interrupt::TG0_WDT_EDGE,
                Interrupt::TG0_LACT_EDGE,
                Interrupt::TG1_T0_EDGE,
                Interrupt::TG1_T1_EDGE,
                Interrupt::TG1_WDT_EDGE,
                Interrupt::TG1_LACT_EDGE,
                Interrupt::SYSTIMER_TARGET0,
                Interrupt::SYSTIMER_TARGET1,
                Interrupt::SYSTIMER_TARGET2,
            ]
            .contains(&interrupt)
        }
    }

    #[cfg(esp32s3)]
    pub(crate) mod chip_specific {
        use super::*;
        #[cfg_attr(place_switch_tables_in_ram, unsafe(link_section = ".rwtext"))]
        pub static INTERRUPT_EDGE: InterruptStatus = InterruptStatus::empty();
        #[inline]
        pub fn interrupt_is_edge(_interrupt: Interrupt) -> bool {
            false
        }
    }
}

#[cfg(feature = "rt")]
mod rt {
    use super::{vectored::*, *};

    #[unsafe(no_mangle)]
    #[ram]
    unsafe fn __level_1_interrupt(save_frame: &mut Context) {
        unsafe {
            handle_interrupts::<1>(save_frame);
        }
    }

    #[unsafe(no_mangle)]
    #[ram]
    unsafe fn __level_2_interrupt(save_frame: &mut Context) {
        unsafe {
            handle_interrupts::<2>(save_frame);
        }
    }

    #[unsafe(no_mangle)]
    #[ram]
    unsafe fn __level_3_interrupt(save_frame: &mut Context) {
        unsafe {
            handle_interrupts::<3>(save_frame);
        }
    }

    #[inline(always)]
    unsafe fn handle_interrupts<const LEVEL: u32>(save_frame: &mut Context) {
        let core = Cpu::current();

        let cpu_interrupt_mask =
            interrupt::get() & interrupt::get_mask() & CPU_INTERRUPT_LEVELS[LEVEL as usize];

        if cpu_interrupt_mask & CPU_INTERRUPT_INTERNAL != 0 {
            // Let's handle CPU-internal interrupts (NMI, Timer, Software, Profiling).
            // These are rarely used by the HAL.

            // Mask the relevant bits
            let cpu_interrupt_mask = cpu_interrupt_mask & CPU_INTERRUPT_INTERNAL;

            // Pick one
            let cpu_interrupt_nr = cpu_interrupt_mask.trailing_zeros();

            // If the interrupt is edge triggered, we need to clear the request on the CPU's
            // side.
            if ((1 << cpu_interrupt_nr) & CPU_INTERRUPT_EDGE) != 0 {
                unsafe {
                    interrupt::clear(1 << cpu_interrupt_nr);
                }
            }

            if let Some(handler) = cpu_interrupt_nr_to_cpu_interrupt_handler(cpu_interrupt_nr) {
                unsafe { handler(save_frame) };
            }
        } else {
            let status = if !cfg!(esp32s3) && (cpu_interrupt_mask & CPU_INTERRUPT_EDGE) != 0 {
                // Next, handle edge triggered peripheral interrupts. Note that on the S3 all
                // peripheral interrupts are level-triggered.

                // If the interrupt is edge triggered, we need to clear the
                // request on the CPU's side
                unsafe { interrupt::clear(cpu_interrupt_mask & CPU_INTERRUPT_EDGE) };

                // For edge interrupts we cannot rely on the peripherals' interrupt status
                // registers, therefore call all registered handlers for current level.
                chip_specific::INTERRUPT_EDGE
            } else {
                // Finally, check level-triggered peripheral sources.
                // These interrupts are cleared by the peripheral.
                status(core)
            };

            let configured_interrupts = configured_interrupts(core, status, LEVEL);
            for interrupt_nr in configured_interrupts.iterator() {
                let handler = unsafe { pac::__INTERRUPTS[interrupt_nr as usize]._handler };
                let handler: fn(&mut Context) = unsafe {
                    core::mem::transmute::<unsafe extern "C" fn(), fn(&mut Context)>(handler)
                };
                handler(save_frame);
            }
        }
    }

    #[inline]
    pub(crate) fn cpu_interrupt_nr_to_cpu_interrupt_handler(
        number: u32,
    ) -> Option<unsafe extern "C" fn(save_frame: &mut Context)> {
        use xtensa_lx_rt::*;
        // we're fortunate that all esp variants use the same CPU interrupt layout
        Some(match number {
            6 => Timer0,
            7 => Software0,
            11 => Profiling,
            14 => NMI,
            15 => Timer1,
            16 => Timer2,
            29 => Software1,
            _ => return None,
        })
    }

    // Raw handlers for CPU interrupts, assembly only.
    unsafe extern "C" {
        fn level4_interrupt(save_frame: &mut Context);
        fn level5_interrupt(save_frame: &mut Context);
        fn level6_interrupt(save_frame: &mut Context);
        fn level7_interrupt(save_frame: &mut Context);
    }

    #[unsafe(no_mangle)]
    #[unsafe(link_section = ".rwtext")]
    unsafe fn __level_4_interrupt(save_frame: &mut Context) {
        unsafe { level4_interrupt(save_frame) }
    }

    #[unsafe(no_mangle)]
    #[unsafe(link_section = ".rwtext")]
    unsafe fn __level_5_interrupt(save_frame: &mut Context) {
        unsafe { level5_interrupt(save_frame) }
    }

    #[unsafe(no_mangle)]
    #[unsafe(link_section = ".rwtext")]
    unsafe fn __level_6_interrupt(save_frame: &mut Context) {
        unsafe { level6_interrupt(save_frame) }
    }

    #[unsafe(no_mangle)]
    #[unsafe(link_section = ".rwtext")]
    unsafe fn __level_7_interrupt(save_frame: &mut Context) {
        unsafe { level7_interrupt(save_frame) }
    }
}
