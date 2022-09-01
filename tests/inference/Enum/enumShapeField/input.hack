enum Suit: string {
    Hearts = "h";
    Diamonds = "d";
    Clubs = "c";
    Spades = "s";
}

function foo(): shape(Suit::Hearts => string) {
    $s = shape();
    $s[Suit::Hearts] = "hello";
    return $s;
}

function baz(): shape(Suit::Hearts => string) {
    $s = shape();
    $s[Suit::Hearts] = "hello";
    return $s;
}

function bar(shape(Suit::Diamonds => int) $s): shape(Suit::Hearts => string, Suit::Diamonds => int) {
    $s[Suit::Hearts] = "hello";
    return $s;
}

function bat(): shape(Suit::Hearts => string, Suit::Clubs => string) {
    return shape(Suit::Hearts => "hello", Suit::Clubs => "goodbye");
}

function bang(shape(Suit::Hearts => string, Suit::Clubs => dict<string, string>) $shape): string {
    return $shape[Suit::Hearts];
}