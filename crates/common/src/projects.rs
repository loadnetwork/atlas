use serde::{Deserialize, Serialize};

// FLPs PIDs
// here this PI_PID is set internally as same as token pid to refer
// as defautl value for wallet with no Set-Delegation
pub const INTERNAL_PI_PID: &str = "4hXj_E-5fAKmo4E8KjgQvuDJKAFk9P2grhycVmISDLs";
pub const PI_PID: &str = "H1I09hGlSlqrvlQid4zBp-lleynE8bNo2Ep1u8xq0fQ";
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

// projects tokens
pub const PI_TOKEN: &str = "4hXj_E-5fAKmo4E8KjgQvuDJKAFk9P2grhycVmISDLs";
pub const LOAD_TOKEN: &str = "gx_jKk-hy8-sB4Wv5WEuvTTVyIRWW3We7rRHthcohBQ";
pub const APUS_TOKEN: &str = "mqBYxpDsolZmJyBdTK8TJp_ftOuIUXVYcSQ8MYZdJg0";
pub const BOTG_TOKEN: &str = "Nx-_Ichdp-9uO_ZKg2DLWPiRlg-DWrSa2uGvINxOjaE";
pub const AOS_TOKEN: &str = "GegJSRSQptBJEF5lcr4XEqWLYFUnNr3_zKQ-P_DnDQs";
pub const WNDR_TOKEN: &str = "7GoQfmSOct_aUOWKM4xbKGg6DzAmOgdKwg8Kf-CbHm4";
pub const ACTION_TOKEN: &str = "OiNYKJ16jP7uj7z0DJO7JZr9ClfioGacpItXTn9fKn8";
pub const SMONEY_TOKEN: &str = "K59Wi9uKXBQfTn3zw7L_t-lwHAoq3Fx-V9sCyOY3dFE";
pub const LQD_TOKEN: &str = "n2MhPK0O3yEvY2zW73sqcmWqDktJxAifJDrri4qireI";
pub const GAME_TOKEN: &str = "s6jcB3ctSbiDNwR-paJgy5iOAhahXahLul8exSLHbGE";
pub const NAU_TOKEN: &str = "5IrQh9aoWTLlLTXogXdGd7FcVubFKOaw7NCRGnkyXCM";
pub const RELLA_TOKEN: &str = "aKmI800gM1Gk12JvwBe2MPxAvXT1ZPfRBxmkUpLJv7g";
pub const ARIO_TOKEN: &str = "qNvAoz0TgcH7DMg8BCVn8jF32QH5L6T29VjHxhHqqGE";
pub const PIXL_TOKEN: &str = "DM3FoZUq_yebASPhgd8pEIRIzDW6muXEhxz5-JwbZwo";
pub const VELA_TOKEN: &str = "kfq7JKVeu-Z9qA0y-0YKXbgNqKJzENqVl0KSrPDOBl4";
pub const INF_TOKEN: &str = "Y2ocP2gBrn4AtodCi1IyoA0X1jCJtx_aKeJddnrHb5U";

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Project {
    pub name: String,
    pub ticker: String,
    pub pid: String,
    pub token: String,
    pub denomination: u32, // todo! add more metadata
}

macro_rules! project {
    ($fn_name:ident, $name:expr, $ticker:expr, $pid:expr, $token:expr, $denomination:expr) => {
        pub fn $fn_name() -> Project {
            Project {
                name: $name.into(),
                ticker: $ticker.into(),
                pid: $pid.into(),
                token: $token.into(),
                denomination: $denomination.into(),
            }
        }
    };
}

impl Project {
    project!(
        pi_internal,
        "Permaweb Index",
        "PI",
        INTERNAL_PI_PID,
        PI_TOKEN,
        12u32
    );
    project!(pi, "Permaweb Index", "PI", PI_PID, PI_TOKEN, 12u32);
    project!(load, "Load Network", "LOAD", LOAD_PID, LOAD_TOKEN, 18u32);
    project!(apus, "Apus Network", "APUS", APUS_PID, APUS_TOKEN, 12u32);
    project!(botega, "Botega Token", "BOTG", BOTG_PID, BOTG_TOKEN, 18u32);
    project!(aos, "AO Strategy", "AOS", AOS_PID, AOS_TOKEN, 18u32);
    project!(wndr, "Wander", "WNDR", WNDR_PID, WNDR_TOKEN, 18u32);
    project!(action, "Action", "ACTION", ACTION_PID, ACTION_TOKEN, 18u32);
    project!(
        space,
        "Space Money",
        "SMONEY",
        SMONEY_PID,
        SMONEY_TOKEN,
        18u32
    );
    project!(lqd, "Liquid Ops", "LQD", LQD_PID, LQD_TOKEN, 18u32);
    project!(game, "ArcAO", "GAME", GAME_PID, GAME_TOKEN, 18u32);
    project!(nau, "Nau", "NAU", NAU_PID, NAU_TOKEN, 18u32);
    project!(
        rella,
        "LLAMMA REBORN",
        "RELLA",
        RELLA_PID,
        RELLA_TOKEN,
        18u32
    );
    project!(ario, "AR.IO", "ARIO", ARIO_PID, ARIO_TOKEN, 6u32);
    project!(pixl, "PIXL Token", "PIXL", PIXL_PID, PIXL_TOKEN, 6u32);
    project!(vela, "Vela", "VELA", VELA_PID, VELA_TOKEN, 18u32);
    project!(inf, "Influence Market", "INF", INF_PID, INF_TOKEN, 18u32);
    // todo! add more active FLPs if any
}

impl Project {
    pub fn is_flp_project(pid: &str) -> bool {
        matches!(
            pid,
            INTERNAL_PI_PID
                | PI_PID
                | LOAD_PID
                | APUS_PID
                | BOTG_PID
                | AOS_PID
                | WNDR_PID
                | ACTION_PID
                | SMONEY_PID
                | LQD_PID
                | GAME_PID
                | NAU_PID
                | RELLA_PID
                | ARIO_PID
                | PIXL_PID
                | VELA_PID
                | INF_PID
        )
    }

    pub fn get_all() -> Vec<Project> {
        vec![
            Project::pi_internal(),
            Project::pi(),
            Project::apus(),
            Project::load(),
            Project::botega(),
            Project::aos(),
            Project::wndr(),
            Project::action(),
            Project::space(),
            Project::lqd(),
            Project::game(),
            Project::nau(),
            Project::rella(),
            Project::ario(),
            Project::pixl(),
            Project::vela(),
            Project::inf(),
        ]
    }
}
