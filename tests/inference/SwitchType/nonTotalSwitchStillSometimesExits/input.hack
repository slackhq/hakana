function takesAnInt(string $str): ?int{
    switch ($str) {
        case "a":
            return 5;

        case "b":
            return null;
    }

    throw new Exception();
}