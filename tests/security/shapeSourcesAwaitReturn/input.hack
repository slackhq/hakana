<<\Hakana\SecurityAnalysis\ShapeSource(
	dict['email' => 'UserSecret'],
)>>
type user_t = shape(
    'id' => int,
    'username' => string,
    'email' => string,
);

async function takesUser() {
    $user = await get_user();
    echo $user['email'];
}

async function get_user(): Awaitable<user_t> {
    return shape('id' => 1, 'username' => 'Matt', 'email' => 'somesecret@gmail.com');
}