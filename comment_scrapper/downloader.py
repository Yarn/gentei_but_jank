#!/usr/bin/env python

# https://github.com/egbertbouman/youtube-comment-downloader/blob/master/youtube_comment_downloader/downloader.py
# 9a15b8e3fbaebad660875409fb1bbe74db17f304

from __future__ import print_function

import argparse
import io
import json
import os
import sys
import time
from contextlib import ExitStack

import re
import requests
from parsel import Selector

from pprint import pprint as stdout_pprint

def eprint(*args, **kwargs):
    print(*args, file=sys.stderr, **kwargs)

def pprint(*args, **kwargs):
    stdout_pprint(*args, stream=sys.stderr, **kwargs)

YOUTUBE_VIDEO_URL = 'https://www.youtube.com/watch?v={youtube_id}'

USER_AGENT = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/79.0.3945.130 Safari/537.36'

SORT_BY_POPULAR = 0
SORT_BY_RECENT = 1

YT_CFG_RE = r'ytcfg\.set\s*\(\s*({.+?})\s*\)\s*;'
YT_INITIAL_DATA_RE = r'(?:window\s*\[\s*["\']ytInitialData["\']\s*\]|ytInitialData)\s*=\s*({.+?})\s*;\s*(?:var\s+meta|</script|\n)'


def regex_search(text, pattern, group=1, default=None):
    match = re.search(pattern, text)
    return match.group(group) if match else default


def ajax_request(session, endpoint, ytcfg, retries=5, sleep=20):
    url = 'https://www.youtube.com' + endpoint['commandMetadata']['webCommandMetadata']['apiUrl']
    
    # print(endpoint['continuationCommand']['token'])
    # input()
    data = {'context': ytcfg['INNERTUBE_CONTEXT'],
            'continuation': endpoint['continuationCommand']['token']}

    for _ in range(retries):
        response = session.post(url, params={'key': ytcfg['INNERTUBE_API_KEY']}, json=data)
        if response.status_code == 200:
            # print(response.text)
            # print(response.text.lower().find('Ug'.lower()))
            # input()
            return response.json()
        if response.status_code in [403, 413]:
            return {}
        else:
            time.sleep(sleep)


def download_comments(youtube_id, sort_by=SORT_BY_RECENT, language=None, sleep=.1, *, goojf):
    session = requests.Session()
    session.headers['User-Agent'] = USER_AGENT
    
    session.cookies.set('CONSENT', 'YES+cb', domain='.youtube.com')
    
    # open url in incognito/private browser window and solve captcha then copy cookie value
    session.cookies.set('goojf', goojf)
    # session.cookies.set('YSC', 'Y---')
    
    url = YOUTUBE_VIDEO_URL.format(youtube_id=youtube_id)
    eprint(url)
    response = session.get(url)
    
    # eprint(response.request.url)
    
    # if 'uxe=' in response.request.url:
    #     session.cookies.set('CONSENT', 'YES+cb', domain='.youtube.com')
    #     response = session.get(YOUTUBE_VIDEO_URL.format(youtube_id=youtube_id))
    
    eprint(' ', response.request.url, ' ')
    html = response.text
    # with open('yt_html.html', 'w') as f:
    #     f.write(html)
    
    json_text = regex_search(html, YT_CFG_RE, default='')
    # eprint('json_text', json_text)
    ytcfg = json.loads(json_text)
    if not ytcfg:
        return # Unable to extract configuration
    if language:
        ytcfg['INNERTUBE_CONTEXT']['client']['hl'] = language

    data = json.loads(regex_search(html, YT_INITIAL_DATA_RE, default=''))
    # print(str(data).lower().find('Ug'.lower()))
    # pprint(data)
    # with open('test_initial.json', 'w') as f:
    #     json.dump(data, f)
    sel = Selector(text=html)
    # chan_a = sel.css('ytd-video-owner-renderer ytd-channel-name a')
    chan_a = sel.css('meta[itemprop="channelId"]')
    eprint()
    eprint('chan_a', chan_a)
    channel_id = chan_a.attrib['content']
    # channel_id = 'chan_temp'
    eprint("channel_id", channel_id)
    chan_b = sel.css('span[itemprop="author"] link[itemprop="name"]')
    channel_name = chan_b.attrib['content']
    eprint("channel_name", channel_name)
    
    channel_info = {
        'channel_id': channel_id,
        'channel_name': channel_name,
    }
    channel_json = json.dumps(channel_info, ensure_ascii=False)
    print(channel_json)

    section = next(search_dict(data, 'itemSectionRenderer'), None)
    renderer = next(search_dict(section, 'continuationItemRenderer'), None) if section else None
    if not renderer:
        # Comments disabled?
        return
    
    # exit_after_one = False
    if '&lc=' in youtube_id:
        comment_id = youtube_id.split('&lc=')[1]
        # exit_after_one = True
    else:
        comment_id = None
    
    needs_sorting = sort_by != SORT_BY_POPULAR
    continuations = [renderer['continuationEndpoint']]
    while continuations:
        continuation = continuations.pop()
        response = ajax_request(session, continuation, ytcfg)
        # pprint(response)
        # with open('test.json', 'w') as f:
        #     json.dump(response, f)
        # sys.exit(1)

        if not response:
            break
        if list(search_dict(response, 'externalErrorMessage')):
            raise RuntimeError('Error returned from server: ' + next(search_dict(response, 'externalErrorMessage')))

        if needs_sorting:
            sort_menu = next(search_dict(response, 'sortFilterSubMenuRenderer'), {}).get('subMenuItems', [])
            if sort_by < len(sort_menu):
                continuations = [sort_menu[sort_by]['serviceEndpoint']]
                needs_sorting = False
                continue
            raise RuntimeError('Failed to set sorting')

        actions = list(search_dict(response, 'reloadContinuationItemsCommand')) + \
                  list(search_dict(response, 'appendContinuationItemsAction'))
        for action in actions:
            for item in action.get('continuationItems', []):
                if action['targetId'] == 'comments-section':
                    # Process continuations for comments and replies.
                    continuations[:0] = [ep for ep in search_dict(item, 'continuationEndpoint')]
                if action['targetId'].startswith('comment-replies-item') and 'continuationItemRenderer' in item:
                    # Process the 'Show more replies' button
                    continuations.append(next(search_dict(item, 'buttonRenderer'))['command'])
        
        for comment in reversed(list(search_dict(response, 'commentRenderer'))):
            if comment_id is not None and comment['commentId'] != comment_id:
                # return
                continue
            # pprint(comment)
            badge = next(search_dict(comment, 'customBadge'), False)
            is_member = False
            if badge:
                # eprint(badge)
                badge_inner = badge['thumbnails'][0]['url']
                if badge_inner:
                    is_member = True
            yield {
                'cid': comment['commentId'],
                'text': ''.join([c['text'] for c in comment['contentText'].get('runs', [])]),
                'time': comment['publishedTimeText']['runs'][0]['text'],
                'author': comment.get('authorText', {}).get('simpleText', ''),
                'channel': comment['authorEndpoint']['browseEndpoint'].get('browseId', ''),
                'votes': comment.get('voteCount', {}).get('simpleText', '0'),
                'photo': comment['authorThumbnail']['thumbnails'][-1]['url'],
                'heart': next(search_dict(comment, 'isHearted'), False),
                'badge': badge,
                'is_member': is_member,
                'channel_id': channel_id,
            }
            # if exit_after_one:
            #     return

        time.sleep(sleep)


def search_dict(partial, search_key):
    stack = [partial]
    while stack:
        current_item = stack.pop()
        if isinstance(current_item, dict):
            for key, value in current_item.items():
                if key == search_key:
                    yield value
                else:
                    stack.append(value)
        elif isinstance(current_item, list):
            for value in current_item:
                stack.append(value)


def main(argv = None):
    parser = argparse.ArgumentParser(add_help=False, description=('Download Youtube comments without using the Youtube API'))
    parser.add_argument('--help', '-h', action='help', default=argparse.SUPPRESS, help='Show this help message and exit')
    parser.add_argument('--youtubeid', '-y', help='ID of Youtube video for which to download the comments')
    parser.add_argument('--output', '-o', help='Output filename (output format is line delimited JSON)')
    parser.add_argument('--limit', '-l', type=int, help='Limit the number of comments', default=1)
    parser.add_argument('--language', '-a', type=str, default=None, help='Language for Youtube generated text (e.g. en)')
    parser.add_argument('--goojf', type=str, default=None, help='goojf cookie')
    parser.add_argument('--sort', '-s', type=int, default=0,#SORT_BY_RECENT,
                        help='Whether to download popular (0) or recent comments (1). Defaults to 0')

    # try:
    args = parser.parse_args() if argv is None else parser.parse_args(argv)

    youtube_id = args.youtubeid
    output = args.output
    limit = args.limit

    if not youtube_id:
        parser.print_usage()
        raise ValueError('you need to specify a Youtube ID and an output filename')

    if output and os.sep in output:
        outdir = os.path.dirname(output)
        if not os.path.exists(outdir):
            os.makedirs(outdir)

    eprint('Downloading Youtube comments for video:', youtube_id)
    count = 0
    # with io.open(output, 'w', encoding='utf8') as fp:
    with ExitStack() as stack:
        if output:
            fp = stack.enter_context(io.open(output, 'w', encoding='utf8'))
        sys.stderr.write('Downloaded %d comment(s)\r' % count)
        sys.stderr.flush()
        start_time = time.time()
        for comment in download_comments(youtube_id, args.sort, args.language, goojf=args.goojf):
            comment_json = json.dumps(comment, ensure_ascii=False)
            if output:
                print(comment_json.decode('utf-8') if isinstance(comment_json, bytes) else comment_json, file=fp)
            count += 1
            sys.stderr.write('Downloaded %d comment(s)\r' % count)
            sys.stderr.flush()
            pprint(comment)
            print(comment_json)
            if limit and count >= limit:
                break
    eprint('\n[{:.2f} seconds] Done!'.format(time.time() - start_time))

    # except Exception as e:
    #     eprint('Error:', str(e))
    #     sys.exit(1)


if __name__ == "__main__":
    main(sys.argv[1:])
