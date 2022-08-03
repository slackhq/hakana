enum LetterCase: int {
  Upper = 0;
  Lower = 1;
}

function foo(LetterCase $c) {
    switch ($c) {
        case LetterCase::Upper:
            $a = 1;
            break;
        case LetterCase::Lower;
            $a = 2;
            break;
    }
    echo $a;
}