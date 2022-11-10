function checkVecArrayKeyExistsInt(vec<string> $arr): int
{
    if (C\contains_key($arr , 1)) {
        return 0;
    }

    if (C\contains_key($arr , "s")) {
        return 0;
    }

    return 1;
}

function checkDictArrayKeyExistsInt(dict<string, string> $arr): int
{
    if (C\contains_key($arr , "test")) {
        return 0;
    }

    if (C\contains_key($arr , 1)) {
        return 0;
    }

    return 1;
}