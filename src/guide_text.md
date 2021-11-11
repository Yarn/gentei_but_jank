1) Open DMs with by sending the message `>>'dmMe`. You can also send the command `guide` (in a DM) to recieve this guide.

2) Send `new_token`, this will give you a token.

3) Open https://www.youtube.com/watch?v={video_id} and make a comment containing the token.

4) Click on the date of your comment, and copy the url.

5) Back to the bot DM, send `set_comment <url>` where `<url>` is the url copied on the previous step.

>---

6) You need to have your own comment on a video on the talent's channel! Either find an old one you wrote, or write a nice comment on their video. Avoid leaving empty or spam comments since they may get deleted or you may get shadow banned.

You can go to YouTube -> hamburger (top left) -> History -> select "Community" radio button, then click Comments and it'll bring you to a google activity page that lists comments you've left.

7) Same thing again, click on the date of your comment, and copy the url.

8) Back to the bot DM, send `set_comment <url>` where `<url>` is the url copied on the previous step.

>---

You can delete your comment containing your token once it's verified. You must keep your comments on membered channels.

You can run commands outside DMs by prefixing them with `>>'`

You can get a list of available commands using the `help` command. Commands shown with a / prefix are probably not actually available as slash commands and need to be used with a `>>'` prefix.

You can check the status of your comments using the `status` command.

You can use `clear_token <url>` to remove a channel from your list. `<url>` is a link to the *channel* you want to remove.

You can get help or report errors in the {support_text}

>---

If you want to link multiple youtube accounts to a single youtube channel/discord account you need to do `new_token {channel_id}'1` (1 can be replaced with a higher number for additional accounts) then `set_comment_b new_token {channel_id}'1 <video id> <comment id>` (a link to a comment is of the form `https://www.youtube.com/watch?v=<video id>&lc=<comment_id>`)

You can link up to 3 discord accounts to one youtube account.
