class C {
    const A = 1;
    const B = -1;
}

const A = 1;
const B = -1;

$i = rand(0, 1) ? A : B;
if (rand(0, 1)) {
    $i = 0;
}

if ($i === A) {
    echo "here";
} else if ($i === B) {
    echo "here";
}

$i = rand(0, 1) ? C::A : C::B;

if (rand(0, 1)) {
    $i = 0;
}

if ($i === C::A) {
    echo "here";
} else if ($i === C::B) {
    echo "here";
}