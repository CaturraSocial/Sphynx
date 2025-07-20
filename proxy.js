const url  = require('url');
const proxyErrors = require('./proxyErrors');
const proxyHttp = require('./proxyHttp');

const proxyHandler = (requestUrl, response) => {
    let Url;
    let {badRequestError, serverError} = proxyErrors;
    try {
        Url = url.parse(requestUrl.query.url);
        if (!['http:','https:'].includes(Url.protocol)) throw('Unsupported protocol');
    } 
    catch(err) {
        if(Url === undefined){
            response.writeHead(200, { 'Content-Type': 'text/html' });
            response.write('<html><body><h1>Hello, Welcome to the Sphynx Proxy server.</h1></body></html>');
            response.end();
            return;
        }else{
            return badRequestError(`Unable to parse URL: ${requestUrl.query.url}`, response);
        }
    }
    const Fetch = new Promise((resolve, fail) => { 
        try {
            proxyHttp.fetchRemote(Url, resolve, fail);
        } 
        catch(e) {
            serverError(e, response);
            fail(e);
        }
    });
    Fetch
        .then(remoteResponse => {
            pipeRemote(remoteResponse, response);
        })
        .catch(error => {
            if (!response.finished) badRequestError(error, response);
        });
}

const imageProxyHandler = (requestUrl, response) => {
    let imageUrl;
    let {badRequestError, serverError} = proxyErrors;
    try {
        imageUrl = url.parse(requestUrl.query.url);
        if (!['http:','https:'].includes(imageUrl.protocol)) throw('Unsupported protocol');
    } 
    catch(err) {
        return badRequestError(`Unable to parse URL: ${requestUrl.query.url}`,
            response);
    }
    const fetchImage = new Promise((resolve, fail) => { 
        try {
            proxyHttp.fetchRemoteImage(imageUrl, resolve, fail);
        } 
        catch(e) {
            serverError(e, response);
            fail(e);
        }
    });
    fetchImage
        .then(remoteImageResponse => {
            pipeRemote(remoteImageResponse, response);
        })
        .catch(error => {
            if (!response.finished) badRequestError(error, response);
        });
}

const audioProxyHandler = (requestUrl, response) => {
    let audioUrl;
    let {badRequestError, serverError} = proxyErrors;
    try {
        audioUrl = url.parse(requestUrl.query.url);
        if (!['http:','https:'].includes(audioUrl.protocol)) throw('Unsupported protocol');
    } 
    catch(err) {
        return badRequestError(`Unable to parse URL: ${requestUrl.query.url}`,
            response);
    }
    const fetchAudio = new Promise((resolve, fail) => { 
        try {
            proxyHttp.fetchRemoteAudio(audioUrl, resolve, fail);
        } 
        catch(e) {
            serverError(e, response);
            fail(e);
        }
    });
    fetchAudio
        .then(remoteAudioResponse => {
            pipeRemote(remoteAudioResponse, response);
        })
        .catch(error => {
            if (!response.finished) badRequestError(error, response);
        });
}

const videoProxyHandler = (requestUrl, response) => {
    let videoUrl;
    let {badRequestError, serverError} = proxyErrors;
    try {
        videoUrl = url.parse(requestUrl.query.url);
        if (!['http:','https:'].includes(videoUrl.protocol)) throw('Unsupported protocol');
    } 
    catch(err) {
        return badRequestError(`Unable to parse URL: ${requestUrl.query.url}`,
            response);
    }
    const fetchVideo = new Promise((resolve, fail) => { 
        try {
            proxyHttp.fetchRemoteVideo(videoUrl, resolve, fail);
        } 
        catch(e) {
            serverError(e, response);
            fail(e);
        }
    });
    fetchVideo
        .then(remoteVideoResponse => {
            pipeRemote(remoteVideoResponse, response);
        })
        .catch(error => {
            if (!response.finished) badRequestError(error, response);
        });
}

const pipeRemote = (remoteResponse, response) => {
    response.writeHead(200, remoteResponse.headers);
    remoteResponse.pipe(response, { end: true });
}

module.exports = { proxyHandler, imageProxyHandler, audioProxyHandler, videoProxyHandler };
