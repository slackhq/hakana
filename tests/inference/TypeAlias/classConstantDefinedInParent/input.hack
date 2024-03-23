enum Suit: string {
    Hearts = "h";
    Diamonds = "d";
    Clubs = "c";
    Spades = "s";
}

abstract class A {
  abstract const type T as arraykey;
  abstract const this::T MY_ENUM;
}

abstract class B extends A {
  const type T = Suit;
}

function get_suit(B $b): Suit {
  return $b::MY_ENUM;
}