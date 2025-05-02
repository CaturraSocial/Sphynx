const http = require('http');
const https = require('https'); 
const url  = require('url');

const MAX_REQUESTS = 5;

/**
 * Fetches a remote resource.
 * 
 * @param {url.URL} Url - The URL of the remote resource.
 * @param {Function} resolve - Callback function to handle successful responses.
 * @param {Function} fail - Callback function to handle errors during the request process.
 * @param {number} [requestsMade=0] - The current count of requests made, used to prevent infinite redirection loops.
 */
const fetchRemote = (Url, resolve, fail, requestsMade=0) => { 
    const fetchModule = Url.protocol === 'https:' ? https : http;
    const remoteRequest = fetchModule.get(Url.href, remoteResponse => {
        if ([301,302].includes(remoteResponse.statusCode)) {
            return makeRecursiveRequest(remoteResponse, resolve, fail, requestsMade);
        } else {
            if (remoteResponse.statusCode !== 200) {
                return fail(`Unsupported remote server status code ${remoteResponse.statusCode}`);
            }
        }
        if (!remoteResponse.headers['content-type'].includes('image/') && !remoteResponse.headers['content-type'].includes('audio/') && !remoteResponse.headers['content-type'].includes('video/')){
            return fail(`Unsupported content type ${remoteResponse.headers['content-type']}`);
        }
        return resolve(remoteResponse);  
    });
    remoteRequest.on('error', function() {
        fail('Error contacting remote server');
    });
}

/**
 * Fetches a remote image resource.
 * 
 * @param {url.URL} imageUrl - The URL of the remote image resource.
 * @param {Function} resolve - Callback function to handle successful responses.
 * @param {Function} fail - Callback function to handle errors during the request process.
 * @param {number} [requestsMade=0] - The current count of requests made, used to prevent infinite redirection loops.
 */
const fetchRemoteImage = (imageUrl, resolve, fail, requestsMade=0) => { 
    const fetchModule = imageUrl.protocol === 'https:' ? https : http;
    const remoteRequest = fetchModule.get(imageUrl.href, remoteResponse => {
        if ([301,302].includes(remoteResponse.statusCode)) {
            return makeRecursiveRequest(remoteResponse, resolve, fail, requestsMade);
        } else {
            if (remoteResponse.statusCode !== 200) {
                return fail(`Unsupported remote server status code ${remoteResponse.statusCode}`);
            }
        }
        if (!remoteResponse.headers['content-type'].includes('image/')){
            return fail(`Unsupported content type ${remoteResponse.headers['content-type']}`);
        }
        return resolve(remoteResponse);  
    });
    remoteRequest.on('error', function() {
        fail('Error contacting remote server');
    });
}

/**
 * Fetches a remote audio resource.
 * 
 * @param {url.URL} audioUrl - The URL of the remote audio resource.
 * @param {Function} resolve - Callback function to handle successful responses.
 * @param {Function} fail - Callback function to handle errors during the request process.
 * @param {number} [requestsMade=0] - The current count of requests made, used to prevent infinite redirection loops.
 */
const fetchRemoteAudio = (audioUrl, resolve, fail, requestsMade=0) => { 
    const fetchModule = audioUrl.protocol === 'https:' ? https : http;
    const remoteRequest = fetchModule.get(audioUrl.href, remoteResponse => {
        if ([301,302].includes(remoteResponse.statusCode)) {
            return makeRecursiveRequest(remoteResponse, resolve, fail, requestsMade);
        } else {
            if (remoteResponse.statusCode !== 200) {
                return fail(`Unsupported remote server status code ${remoteResponse.statusCode}`);
            }
        }
        if (!remoteResponse.headers['content-type'].includes('audio/')){
            return fail(`Unsupported content type ${remoteResponse.headers['content-type']}`);
        }
        return resolve(remoteResponse);  
    });
    remoteRequest.on('error', function() {
        fail('Error contacting remote server');
    });
}

/**
 * Fetches a remote video resource.
 * 
 * @param {url.URL} videoUrl - The URL of the remote video resource.
 * @param {Function} resolve - Callback function to handle successful responses.
 * @param {Function} fail - Callback function to handle errors during the request process.
 * @param {number} [requestsMade=0] - The current count of requests made, used to prevent infinite redirection loops.
 */
const fetchRemoteVideo = (videoUrl, resolve, fail, requestsMade=0) => { 
    const fetchModule = videoUrl.protocol === 'https:' ? https : http;
    const remoteRequest = fetchModule.get(videoUrl.href, remoteResponse => {
        if ([301,302].includes(remoteResponse.statusCode)) {
            return makeRecursiveRequest(remoteResponse, resolve, fail, requestsMade);
        } else {
            if (remoteResponse.statusCode !== 200) {
                return fail(`Unsupported remote server status code ${remoteResponse.statusCode}`);
            }
        }
        if (!remoteResponse.headers['content-type'].includes('video/')){
            return fail(`Unsupported content type ${remoteResponse.headers['content-type']}`);
        }
        return resolve(remoteResponse);  
    });
    remoteRequest.on('error', function() {
        fail('Error contacting remote server');
    });
}

/**
 * Handles HTTP redirects by recursively making requests to the provided redirect URL.
 * 
 * @param {http.IncomingMessage} remoteResponse - The response object from the initial request containing redirect information.
 * @param {Function} resolve - Callback function to handle successful responses.
 * @param {Function} fail - Callback function to handle errors during the request process.
 * @param {number} requestsMade - The current count of requests made, used to prevent infinite redirection loops.
 */
const makeRecursiveRequest = (remoteResponse, resolve, fail, requestsMade) => {
    requestsMade++;
    if (requestsMade >= MAX_REQUESTS) {
        return fail('Too many requests made');
    }
    const redirectUrl = url.parse(remoteResponse.headers.location);
    console.log(`Response specifies redirect to ${redirectUrl.href},\
  ${requestsMade} requests made`);
    return fetchRemote(redirectUrl, resolve, fail, requestsMade),
            fetchRemoteImage(redirectUrl, resolve, fail, requestsMade),
            fetchRemoteAudio(redirectUrl, resolve, fail, requestsMade),
            fetchRemoteVideo(redirectUrl, resolve, fail, requestsMade);
}

module.exports = { fetchRemote, fetchRemoteImage, fetchRemoteAudio, fetchRemoteVideo };