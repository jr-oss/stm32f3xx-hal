//! # Analog to Digital Converter.
//!
//! ## Examples
//!
//! Check out [examles/adc.rs][].
//!
//! It can be built for the STM32F3Discovery running
//! `cargo build --example adc --features=stm32f303xc`
//!
//! [examples/adc.rs]: https://github.com/stm32-rs/stm32f3xx-hal/blob/v0.8.2/examples/adc.rs

use cortex_m::asm;
use embedded_hal::adc::{Channel, OneShot};

use crate::{
    gpio::{self, Analog},
    pac::{
        adc1::{cfgr::ALIGN_A, smpr1::SMP9_A, smpr2::SMP18_A},
        adc1_2::ccr::CKMODE_A,
        ADC1, ADC1_2, ADC2,
    },
    rcc::{Clocks, AHB},
};

#[cfg(any(
    feature = "stm32f303xb",
    feature = "stm32f303xc",
    feature = "stm32f303xd",
    feature = "stm32f303xe",
))]
use crate::pac::{ADC3, ADC3_4, ADC4};

const MAX_ADVREGEN_STARTUP_US: u32 = 10;

/// Analog Digital Converter Peripheral
// TODO: Remove `pub` from the register block once all functionalities are implemented.
// Leave it here until then as it allows easy access to the registers.
// TODO(Sh3Rm4n) Add configuration and other things like in the `stm32f4xx-hal` crate
pub struct Adc<ADC> {
    /// ADC Register
    adc: ADC,
    clocks: Clocks,
    clock_mode: ClockMode,
    operation_mode: Option<OperationMode>,
}

/// ADC sampling time
///
/// Each channel can be sampled with a different sample time.
/// There is always an overhead of 13 ADC clock cycles.
/// E.g. For Sampletime T_19 the total conversion time (in ADC clock cycles) is
/// 13 + 19 = 32 ADC Clock Cycles
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum SampleTime {
    /// 1.5 ADC clock cycles
    Cycles1C5,
    /// 2.5 ADC clock cycles
    Cycles2C5,
    /// 4.5 ADC clock cycles
    Cycles4C5,
    /// 7.5 ADC clock cycles
    Cycles7C5,
    /// 19.5 ADC clock cycles
    Cycles19C5,
    /// 61.5 ADC clock cycles
    Cycles61C5,
    /// 181.5 ADC clock cycles
    Cycles181C5,
    /// 601.5 ADC clock cycles
    Cycles601C5,
}

impl Default for SampleTime {
    /// [`SampleTime::Cycles1C5`] is also the reset value.
    fn default() -> Self {
        SampleTime::Cycles1C5
    }
}

impl From<SampleTime> for SMP9_A {
    fn from(t: SampleTime) -> Self {
        match t {
            SampleTime::Cycles1C5 => Self::CYCLES1_5,
            SampleTime::Cycles2C5 => Self::CYCLES2_5,
            SampleTime::Cycles4C5 => Self::CYCLES4_5,
            SampleTime::Cycles7C5 => Self::CYCLES7_5,
            SampleTime::Cycles19C5 => Self::CYCLES19_5,
            SampleTime::Cycles61C5 => Self::CYCLES61_5,
            SampleTime::Cycles181C5 => Self::CYCLES181_5,
            SampleTime::Cycles601C5 => Self::CYCLES601_5,
        }
    }
}

impl From<SampleTime> for SMP18_A {
    fn from(t: SampleTime) -> Self {
        match t {
            SampleTime::Cycles1C5 => Self::CYCLES1_5,
            SampleTime::Cycles2C5 => Self::CYCLES2_5,
            SampleTime::Cycles4C5 => Self::CYCLES4_5,
            SampleTime::Cycles7C5 => Self::CYCLES7_5,
            SampleTime::Cycles19C5 => Self::CYCLES19_5,
            SampleTime::Cycles61C5 => Self::CYCLES61_5,
            SampleTime::Cycles181C5 => Self::CYCLES181_5,
            SampleTime::Cycles601C5 => Self::CYCLES601_5,
        }
    }
}

/// ADC operation mode
// TODO: Implement other modes (DMA, Differential,…)
// TODO(Sh3Rm4n): Maybe make operation modes to type states to better
// integrate with embedded-hal crates?
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
pub enum OperationMode {
    /// OneShot Mode
    OneShot,
}

#[derive(Clone, Copy, PartialEq)]
/// ADC Clock Mode
// TODO: Add Asynchronous mode
#[non_exhaustive]
pub enum ClockMode {
    // /// Use Kernel Clock adc_ker_ck_input divided by PRESC. Asynchronous to AHB clock
    // Asynchronous,
    /// Use AHB clock rcc_hclk3. In this case rcc_hclk must equal sys_d1cpre_ck
    SyncDiv1,
    /// Use AHB clock rcc_hclk3 divided by 2
    SyncDiv2,
    /// Use AHB clock rcc_hclk3 divided by 4
    SyncDiv4,
}

impl Default for ClockMode {
    fn default() -> Self {
        ClockMode::SyncDiv2
    }
}

// ADC3_2 returns a pointer to a adc1_2 type, so this from is ok for both.
impl From<ClockMode> for CKMODE_A {
    fn from(clock_mode: ClockMode) -> Self {
        match clock_mode {
            //ClockMode::Asynchronous => CKMODE_A::ASYNCHRONOUS,
            ClockMode::SyncDiv1 => CKMODE_A::SYNCDIV1,
            ClockMode::SyncDiv2 => CKMODE_A::SYNCDIV2,
            ClockMode::SyncDiv4 => CKMODE_A::SYNCDIV4,
        }
    }
}

/// ADC data register alignment
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Align {
    /// Right alignment of output data
    Right,
    /// Left alignment of output data
    Left,
}

impl Default for Align {
    fn default() -> Self {
        Align::Right
    }
}

impl From<Align> for ALIGN_A {
    fn from(align: Align) -> ALIGN_A {
        match align {
            Align::Right => ALIGN_A::RIGHT,
            Align::Left => ALIGN_A::LEFT,
        }
    }
}

/// Maps pins to ADC Channels.
macro_rules! adc_pins {
    ($ADC:ident, $($pin:ty => $chan:expr),+ $(,)*) => {
        $(
            impl Channel<$ADC> for $pin {
                type ID = u8;

                fn channel() -> u8 { $chan }
            }
        )+
    };
}

// # ADC1 Pin/Channel mapping
// ## f303

#[cfg(feature = "svd-f303")]
adc_pins!(ADC1,
    gpio::PA0<Analog> => 1,
    gpio::PA1<Analog> => 2,
    gpio::PA2<Analog> => 3,
    gpio::PA3<Analog> => 4,
    gpio::PC0<Analog> => 6,
    gpio::PC1<Analog> => 7,
    gpio::PC2<Analog> => 8,
    gpio::PC3<Analog> => 9,
);

#[cfg(feature = "gpio-f333")]
adc_pins!(ADC1,
    gpio::PB0<Analog> => 11,
    gpio::PB1<Analog> => 12,
    gpio::PB13<Analog> => 13,
);

#[cfg(any(feature = "gpio-f303", feature = "gpio-f303e"))]
adc_pins!(ADC1,
    gpio::PF4<Analog> => 5,
    gpio::PF2<Analog> => 10,
);

// # ADC2 Pin/Channel mapping
// ## f303

#[cfg(feature = "svd-f303")]
adc_pins!(ADC2,
    gpio::PA4<Analog> => 1,
    gpio::PA5<Analog> => 2,
    gpio::PA6<Analog> => 3,
    gpio::PA7<Analog> => 4,
    gpio::PC4<Analog> => 5,
    gpio::PC0<Analog> => 6,
    gpio::PC1<Analog> => 7,
    gpio::PC2<Analog> => 8,
    gpio::PC3<Analog> => 9,
    gpio::PC5<Analog> => 11,
    gpio::PB2<Analog> => 12,
);

#[cfg(feature = "gpio-f333")]
adc_pins!(ADC2,
    gpio::PB12<Analog> => 13,
    gpio::PB14<Analog> => 14,
    gpio::PB15<Analog> => 15,
);

#[cfg(any(feature = "gpio-f303", feature = "gpio-f303e",))]
adc_pins!(ADC2,
    gpio::PF2<Analog> => 10,
);

// # ADC3 Pin/Channel mapping
// ## f303

#[cfg(any(feature = "gpio-f303", feature = "gpio-f303e",))]
adc_pins!(ADC3,
    gpio::PB1<Analog> => 1,
    gpio::PE9<Analog> => 2,
    gpio::PE13<Analog> => 3,
    // There is no ADC3 Channel #4
    gpio::PB13<Analog> => 5,
    gpio::PE8<Analog> => 6,
    gpio::PD10<Analog> => 7,
    gpio::PD11<Analog> => 8,
    gpio::PD12<Analog> => 9,
    gpio::PD13<Analog> => 10,
    gpio::PD14<Analog> => 11,
    gpio::PB0<Analog> => 12,
    gpio::PE7<Analog> => 13,
    gpio::PE10<Analog> => 14,
    gpio::PE11<Analog> => 15,
    gpio::PE12<Analog> => 16,
);

// # ADC4 Pin/Channel mapping
// ## f303

#[cfg(any(feature = "gpio-f303", feature = "gpio-f303e",))]
adc_pins!(ADC4,
    gpio::PE14<Analog> => 1,
    gpio::PE15<Analog> => 2,
    gpio::PB12<Analog> => 3,
    gpio::PB14<Analog> => 4,
    gpio::PB15<Analog> => 5,
    gpio::PE8<Analog> => 6,
    gpio::PD10<Analog> => 7,
    gpio::PD11<Analog> => 8,
    gpio::PD12<Analog> => 9,
    gpio::PD13<Analog> => 10,
    gpio::PD14<Analog> => 11,
    gpio::PD8<Analog> => 12,
    gpio::PD9<Analog> => 13,
);

// Abstract implementation of ADC functionality
// Do not use directly. See adc12_hal for a applicable Macro.
// TODO: Extend/generalize beyond f303
macro_rules! adc_hal {
    ($(
            $ADC:ident: ($adcx:ident, $ADC_COMMON:ident),
    )+) => {
        $(
            impl Adc<$ADC> {

                /// Init a new ADC
                ///
                /// Enables the clock, performs a calibration and enables the ADC
                ///
                /// # Panics
                /// If one of the following occurs:
                /// * the clocksetting is not well defined.
                /// * the clock was already enabled with a different setting
                ///
                pub fn $adcx(
                    adc: $ADC,
                    adc_common : &mut $ADC_COMMON,
                    ahb: &mut AHB,
                    clock_mode: ClockMode,
                    clocks: Clocks,
                ) -> Self {
                    let mut this_adc = Self {
                        adc,
                        clocks,
                        clock_mode,
                        operation_mode: None,
                    };
                    if !(this_adc.clocks_welldefined(clocks)) {
                        crate::panic!("Clock settings not well defined");
                    }
                    if !(this_adc.enable_clock(ahb, adc_common)){
                        crate::panic!("Clock already enabled with a different setting");
                    }
                    this_adc.set_align(Align::default());
                    this_adc.calibrate();
                    // ADEN bit cannot be set during ADCAL=1
                    // and 4 ADC clock cycle after the ADCAL
                    // bit is cleared by hardware
                    this_adc.wait_adc_clk_cycles(4);
                    this_adc.enable();

                    this_adc
                }

                /// Releases the ADC peripheral and associated pins
                pub fn free(mut self) -> $ADC {
                    self.disable();
                    self.adc
                }

                /// Software can use ClockMode::SyncDiv1 only if
                /// hclk and sysclk are the same. (see reference manual 15.3.3)
                fn clocks_welldefined(&self, clocks: Clocks) -> bool {
                    if (self.clock_mode == ClockMode::SyncDiv1) {
                        clocks.hclk().0 == clocks.sysclk().0
                    } else {
                        true
                    }
                }

                /// sets up adc in one shot mode for a single channel
                pub fn setup_oneshot(&mut self) {
                    self.adc.cr.modify(|_, w| w.adstp().stop());
                    self.adc.isr.modify(|_, w| w.ovr().clear());

                    self.adc.cfgr.modify(|_, w| w
                        .cont().single()
                        .ovrmod().preserve()
                    );

                    self.set_sequence_len(1);

                    self.operation_mode = Some(OperationMode::OneShot);
                }

                fn set_sequence_len(&mut self, len: u8) {
                    crate::assert!(len - 1 < 16, "ADC sequence length must be in 1..=16");
                    self.adc.sqr1.modify(|_, w| w.l().bits(len - 1));
                }

                fn set_align(&self, align: Align) {
                    self.adc.cfgr.modify(|_, w| w.align().variant(align.into()));
                }

                /// Software procedure to enable the ADC
                /// According to RM0316 15.3.9
                fn enable(&mut self) {
                    // This check assumes, that the ADC was enabled before and it was waited until
                    // ADRDY=1 was set.
                    // This assumption is true, if the peripheral was initially enabled through
                    // this method.
                    if !self.adc.cr.read().aden().is_enable() {
                        // Set ADEN=1
                        self.adc.cr.modify(|_, w| w.aden().enable());
                        // Wait until ADRDY=1 (ADRDY is set after the ADC startup time). This can be
                        // done using the associated interrupt (setting ADRDYIE=1).
                        while self.adc.isr.read().adrdy().is_not_ready() {}
                    }
                }

                /// Disable according to RM0316 15.3.9
                fn disable(&mut self) {
                    // NOTE: Software is allowed to set ADSTP only when ADSTART=1 and ADDIS=0
                    // (ADC is enabled and eventually converting a regular conversion and there is no
                    // pending request to disable the ADC)
                    if self.adc.cr.read().addis().bit() == false
                        && (self.adc.cr.read().adstart().bit() || self.adc.cr.read().jadstart().bit()) {
                        self.adc.cr.modify(|_, w| w.adstp().stop());
                        // NOTE: In auto-injection mode (JAUTO=1), setting ADSTP bit aborts both
                        // regular and injected conversions (do not use JADSTP)
                        if !self.adc.cfgr.read().jauto().is_enabled() {
                            self.adc.cr.modify(|_, w| w.jadstp().stop());
                        }
                        while self.adc.cr.read().adstp().bit() || self.adc.cr.read().jadstp().bit() { }
                    }

                    // NOTE: Software is allowed to set ADDIS only when ADEN=1 and both ADSTART=0
                    // and JADSTART=0 (which ensures that no conversion is ongoing)
                    if self.adc.cr.read().aden().is_enable() {
                        self.adc.cr.modify(|_, w| w.addis().disable());
                        while self.adc.cr.read().addis().bit() { }
                    }
                }

                /// Calibrate according to RM0316 15.3.8
                fn calibrate(&mut self) {
                    if !self.adc.cr.read().advregen().is_enabled() {
                        self.advregen_enable();
                        self.wait_advregen_startup();
                    }

                    self.disable();

                    self.adc.cr.modify(|_, w| w
                        .adcaldif().single_ended()
                        .adcal()   .calibration());

                    while self.adc.cr.read().adcal().is_calibration() {}
                }

                fn wait_adc_clk_cycles(&self, cycles: u32) {
                    // using a match statement here so compilation will fail once asynchronous clk
                    // mode is implemented (CKMODE[1:0] = 00b).  This will force whoever is working
                    // on it to rethink what needs to be done here :)
                    let adc_per_cpu_cycles = match self.clock_mode {
                        ClockMode::SyncDiv1 => 1,
                        ClockMode::SyncDiv2 => 2,
                        ClockMode::SyncDiv4 => 4,
                    };
                    asm::delay(adc_per_cpu_cycles * cycles);
                }

                fn advregen_enable(&mut self){
                    // need to go through intermediate first
                    self.adc.cr.modify(|_, w| w.advregen().intermediate());
                    self.adc.cr.modify(|_, w| w.advregen().enabled());
                }

                /// wait for the advregen to startup.
                ///
                /// This is based on the MAX_ADVREGEN_STARTUP_US of the device.
                fn wait_advregen_startup(&self) {
                    asm::delay((MAX_ADVREGEN_STARTUP_US * 1_000_000) / self.clocks.sysclk().0);
                }

                /// busy ADC read
                fn convert_one(&mut self, chan: u8) -> u16 {
                    if self.operation_mode != Some(OperationMode::OneShot) {
                        self.setup_oneshot();
                    }
                    self.set_chan_smps(chan, SampleTime::default());
                    self.select_single_chan(chan);

                    self.adc.cr.modify(|_, w| w.adstart().start());
                    while self.adc.isr.read().eos().is_not_complete() {}
                    self.adc.isr.modify(|_, w| w.eos().clear());
                    return self.adc.dr.read().rdata().bits();
                }

                /// This should only be invoked with the defined channels for the particular
                /// device. (See Pin/Channel mapping above)
                fn select_single_chan(&self, chan: u8) {
                    self.adc.sqr1.modify(|_, w|
                        // NOTE(unsafe): chan is the x in ADCn_INx
                        unsafe { w.sq1().bits(chan) }
                    );
                }

                /// Note: only allowed when ADSTART = 0
                // TODO: there are boundaries on how this can be set depending on the hardware.
                fn set_chan_smps(&self, chan: u8, smp: SampleTime) {
                    match chan {
                        1 => self.adc.smpr1.modify(|_, w| w.smp1().variant(smp.into())),
                        2 => self.adc.smpr1.modify(|_, w| w.smp2().variant(smp.into())),
                        3 => self.adc.smpr1.modify(|_, w| w.smp3().variant(smp.into())),
                        4 => self.adc.smpr1.modify(|_, w| w.smp4().variant(smp.into())),
                        5 => self.adc.smpr1.modify(|_, w| w.smp5().variant(smp.into())),
                        6 => self.adc.smpr1.modify(|_, w| w.smp6().variant(smp.into())),
                        7 => self.adc.smpr1.modify(|_, w| w.smp7().variant(smp.into())),
                        8 => self.adc.smpr1.modify(|_, w| w.smp8().variant(smp.into())),
                        9 => self.adc.smpr1.modify(|_, w| w.smp9().variant(smp.into())),
                        10 => self.adc.smpr2.modify(|_, w| w.smp10().variant(smp.into())),
                        11 => self.adc.smpr2.modify(|_, w| w.smp11().variant(smp.into())),
                        12 => self.adc.smpr2.modify(|_, w| w.smp12().variant(smp.into())),
                        13 => self.adc.smpr2.modify(|_, w| w.smp13().variant(smp.into())),
                        14 => self.adc.smpr2.modify(|_, w| w.smp14().variant(smp.into())),
                        15 => self.adc.smpr2.modify(|_, w| w.smp15().variant(smp.into())),
                        16 => self.adc.smpr2.modify(|_, w| w.smp16().variant(smp.into())),
                        17 => self.adc.smpr2.modify(|_, w| w.smp17().variant(smp.into())),
                        18 => self.adc.smpr2.modify(|_, w| w.smp18().variant(smp.into())),
                        _ => crate::unreachable!(),
                    };
                }

                /// Get access to the underlying register block.
                ///
                /// # Safety
                ///
                /// This function is not _memory_ unsafe per se, but does not guarantee
                /// anything about assumptions of invariants made in this implementation.
                ///
                /// Changing specific options can lead to un-expected behavior and nothing
                /// is guaranteed.
                pub unsafe fn peripheral(&mut self) -> &mut $ADC {
                    &mut self.adc
                }

            }

            impl<Word, Pin> OneShot<$ADC, Word, Pin> for Adc<$ADC>
            where
                Word: From<u16>,
                Pin: Channel<$ADC, ID = u8>,
                {
                    type Error = ();

                    fn read(&mut self, _pin: &mut Pin) -> nb::Result<Word, Self::Error> {
                        // TODO: Convert back to previous mode after use.
                        let res = self.convert_one(Pin::channel());
                        return Ok(res.into());
                    }
                }
        )+
    }
}

// Macro to implement ADC functionallity for ADC1 and ADC2
// TODO: Extend/differentiate beyond f303.
macro_rules! adc12_hal {
    ($(
            $ADC:ident: ($adcx:ident),
    )+) => {
        $(
            impl Adc<$ADC> {
                /// Returns true iff
                ///     the clock can be enabled with the given settings
                ///  or the clock was already enabled with the same settings
                fn enable_clock(&self, ahb: &mut AHB, adc_common: &mut ADC1_2) -> bool {
                    if ahb.enr().read().adc12en().is_enabled() {
                        return (adc_common.ccr.read().ckmode().variant() == self.clock_mode.into());
                    }
                    ahb.enr().modify(|_, w| w.adc12en().enabled());
                    adc_common.ccr.modify(|_, w| w
                        .ckmode().variant(self.clock_mode.into())
                    );
                    true
                }
            }
            adc_hal! {
                $ADC: ($adcx, ADC1_2),
            }
        )+
    }
}

// Macro to implement ADC functionallity for ADC3 and ADC4
// TODO: Extend/differentiate beyond f303.
#[cfg(any(feature = "gpio-f303", feature = "gpio-f303e",))]
macro_rules! adc34_hal {
    ($(
            $ADC:ident: ($adcx:ident),
    )+) => {
        $(
            impl Adc<$ADC> {
                /// Returns true iff
                ///     the clock can be enabled with the given settings
                ///  or the clock was already enabled with the same settings
                fn enable_clock(&self, ahb: &mut AHB, adc_common: &mut ADC3_4) -> bool {
                    if ahb.enr().read().adc34en().is_enabled() {
                        return (adc_common.ccr.read().ckmode().variant() == self.clock_mode.into());
                    }
                    ahb.enr().modify(|_, w| w.adc34en().enabled());
                    adc_common.ccr.modify(|_, w| w
                        .ckmode().variant(self.clock_mode.into())
                    );
                    true
                }
            }
            adc_hal! {
                $ADC: ($adcx, ADC3_4),
            }
        )+
    }
}

#[cfg(feature = "svd-f303")]
adc12_hal! {
    ADC1: (adc1),
    ADC2: (adc2),
}
#[cfg(any(feature = "gpio-f303", feature = "gpio-f303e",))]
adc34_hal! {
    ADC3: (adc3),
    ADC4: (adc4),
}
