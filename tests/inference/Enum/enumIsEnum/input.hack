enum A: string as string {
    ONE = 'one';
    TWO = 'two';
}

enum B: string as string {
    ONE = A::ONE;
    TWO = B::ONE;
    THREE = 'three';
}

function takes_a(A $a): void {
    if ($a is B) {
        echo $a;
    }
    echo $a as B;
}

function takes_b(B $b): void {
    echo $b is A ? $b : '';
    echo $b as A;
}

function takes_b_again(B $b): void {
    if ($b !== B::ONE) {
        echo $b as A;
    }
}
