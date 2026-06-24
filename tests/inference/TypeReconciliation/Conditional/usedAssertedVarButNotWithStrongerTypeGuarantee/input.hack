function broken(bool $b, ?User $u) : void {
    if ($b || (rand(0, 1) !== 0 && (!$u || takesUser($u)))) {
        return;
    }

    if ($u) {}
}

final class User {}

function takesUser(User $a) : bool {
    return true;
}