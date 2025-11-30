use serde::{Deserialize, Serialize};

// FLPs PIDs
pub const PI_PID: &str = "4hXj_E-5fAKmo4E8KjgQvuDJKAFk9P2grhycVmISDLs";
pub const APUS_PID: &str = "jHZBsy0SalZ6I5BmYKRUt0AtLsn-FCFhqf_n6AgwGlc";
pub const LOAD_PID: &str = "Qz3n2P-EiWNoWsvk7gKLtrV9ChvSXQ5HJPgPklWEgQ0";
pub const BOTG_PID: &str = "UcBPqkaVI7W4I_YMznrt2JUoyc_7TScCdZWOOSBvMSU";
pub const AOS_PID: &str = "t7_efxAUDftIEl9QfBi0KYSz8uHpMS81xfD3eqd89rQ";
pub const WNDR_PID: &str = "11T2aA8M-ZcoEnDqG37Kf2dzEGY2r4_CyYeiN_1VTvU";
pub const ACTION_PID: &str = "NXZjrPKh-fQx8BUCG_OXBUtB4Ix8Xf0gbUtREFoWQ2Q";
pub const SMONEY_PID: &str = "oIuISObCStjTFMnV3CrrERRb9KTDGN4507-ARysYzLE";
pub const LQD_PID: &str = "N0L1lUC-35wgyXK31psEHRjySjQMWPs_vHtTas5BJa8";
pub const GAME_PID: &str = "nYHhoSEtelyL3nQ6_CFoOVnZfnz2VHK-nEez962YMm8";
pub const NAU_PID: &str = "oTkFjTiRUKGp-Lk1YduBDTRRc7j1dM0W_bTgp5Aach8";
pub const RELLA_PID: &str = "_L_GMvgax750A8oORtNPetcmq5fog3K6WtvY4PFpipo";
pub const ARIO_PID: &str = "rW7h9J9jE2Xp36y4SKn2HgZaOuzRmbMfBRPwrFFifHE";
pub const PIXL_PID: &str = "3eZ6_ry6FD9CB58ImCQs6Qx_rJdDUGhz-D2W1AqzHD8";
pub const VELA_PID: &str = "8TRsYFzbhp97Er5bFJL4Xofa4Txv4fv8S0szEscqopU";
pub const INF_PID: &str = "LnFIQUwAdMZ9LEWlfQ7VZ3zJOW-0p8Irc_2gAVshs3w";


#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Project {
    pub name: String,
    pub ticker: String,
    pub pid: String,
    // todo! add more metadata
}

macro_rules! project {
    ($fn_name:ident, $name:expr, $ticker:expr, $pid:expr) => {
        pub fn $fn_name() -> Project {
            Project {
                name: $name.into(),
                ticker: $ticker.into(),
                pid: $pid.into(),
            }
        }
    };
}

impl Project {
    project!(pi, "Permaweb Index", "PI", PI_PID);
    project!(load, "Load Network", "LOAD", LOAD_PID);
    project!(apus, "Apus Network", "APUS", APUS_PID);
    project!(botega, "Botega Token", "BOTG", BOTG_PID);
    project!(aos, "AO Strategy", "AOS", AOS_PID);
    project!(wndr, "Wander", "WNDR", WNDR_PID);
    project!(action, "Action", "ACTION", ACTION_PID);
    project!(space, "Space Money", "SMONEY", SMONEY_PID);
    project!(lqd, "Liquid Ops", "LQD", LQD_PID);
    project!(game, "ArcAO", "GAME", GAME_PID);
    project!(nau, "Nau", "NAU", NAU_PID);
    project!(rella, "LLAMMA REBORN", "RELLA", RELLA_PID);
    project!(ario, "AR.IO", "ARIO", ARIO_PID);
    project!(pixl, "PIXL Token", "PIXL", PIXL_PID);
    project!(vela, "Vela", "VELA", VELA_PID);
    project!(inf, "Influence Market", "INF", INF_PID);
    // todo! add more active FLPs if any
}
