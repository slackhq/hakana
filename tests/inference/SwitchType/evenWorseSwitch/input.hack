function foo(string $locale) : int {
    switch ($locale) {
        case "af":
        case "af_ZA":
        case "bn":
        case "bn_BD":
        case "bn_IN":
        case "bg":
        case "bg_BG":
        case "ca":
        case "ca_AD":
        case "ca_ES":
        case "ca_FR":
        case "ca_IT":
        case "da":
        case "da_DK":
        case "de":
        case "de_AT":
        case "de_BE":
        case "de_CH":
        case "de_DE":
        case "de_LI":
        case "de_LU":
        case "el":
        case "el_CY":
        case "el_GR":
        case "en":
        case "en_AG":
        case "en_AU":
        case "en_BW":
        case "en_CA":
        case "en_DK":
        case "en_GB":
        case "en_HK":
        case "en_IE":
        case "en_IN":
        case "en_NG":
        case "en_NZ":
        case "en_PH":
        case "en_SG":
        case "en_US":
        case "en_ZA":
        case "en_ZM":
        case "en_ZW":
        case "es_VE":
            return 3;
    }

    return 4;
}