<<\Hakana\SecurityAnalysis\ShapeSource(
	dict[
		'name' => 'RawUserData',
	],
)>>
type user_t = shape(
    'name' => string
);

interface User {
    public function getName(): string;
}

abstract class BaseUser {
    public function __construct(private user_t $data) {}

    public function getName(): string {
        return $this->data['name'];
    }
}

final class FooUser extends BaseUser implements User {}

function take_user(User $user): void {
    echo $user->getName();
}