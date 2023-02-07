enum Test: string {
    FOO = 'FOO';
}

function checkDictArrayKeyExistsInt(dict<string, Test> $arr, string $foobar): int
{
    if (C\contains($arr , "FOO")) {
        return 0;
    }

    if (C\contains($arr, "BAR")) {
        return 0;
    }

    $arr2 = dict["test" => Test::FOO];
    if (C\contains($arr2, $foobar)) {
        return 0;
    }

    return 1;
}