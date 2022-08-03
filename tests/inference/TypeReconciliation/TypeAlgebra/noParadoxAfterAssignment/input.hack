function get_bool(): bool {
    return rand() % 2 > 0;
}

function leftover(): bool {
    $res = get_bool();
    if ($res === false) {
        return true;
    }
    $res = get_bool();
    if ($res === false) {
        return false;
    }
    return true;
}