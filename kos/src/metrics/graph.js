
var canvas = document.querySelector("canvas"),
    context = canvas.getContext("2d");
let x, y, line, area;
let margin = {top: 20, right: 20, bottom: 30, left: 50};
let size = {width:0, height:0};
let parseTime = d3.timeParse("%d-%b-%y");

function init(){
    let rect = canvas.getBoundingClientRect();
    let pixelRatio = window.devicePixelRatio||1;

    canvas.width = Math.round(pixelRatio * rect.right)
                - Math.round(pixelRatio * rect.left);
    canvas.height = Math.round(pixelRatio * rect.bottom)
                - Math.round(pixelRatio * rect.top);

    build();
}

function build(){

    size.width = canvas.width - margin.left - margin.right;
    size.height = canvas.height - margin.top - margin.bottom;

    x = d3.scaleTime()
        .range([0, size.width]);

    y = d3.scaleLinear()
        .range([size.height, 0]);

    line = d3.line()
        .x(function(d) { return x(d.date); })
        .y(function(d) { return y(d.close); })
        .curve(d3.curveStep)
        .context(context);
        
    area = d3.area()
        .x(function(d) { return x(d.date); })
        .y0(size.height)
        .y1(function(d) { return y(d.close); })
        .context(context);

    context.translate(margin.left, margin.top);
}
init();

d3.tsv("/resources/data.tsv", function(d) {
    d.date = parseTime(d.date);
    d.close = +d.close;
    return d;
}).then(function(data, error) {
    

    x.domain(d3.extent(data, function(d) { return d.date; }));
    y.domain(d3.extent(data, function(d) { return d.close; }));

    xAxis();
    yAxis();

    context.beginPath();
    area(data);
    context.fillStyle = 'red';
    context.strokeStyle = 'red';
    context.fill();

});

function xAxis() {
    var tickCount = 10,
    tickSize = 6,
    ticks = x.ticks(tickCount),
    tickFormat = x.tickFormat();

    context.beginPath();
    ticks.forEach(function(d) {
        context.moveTo(x(d), size.height);
        context.lineTo(x(d), size.height + tickSize);
    });
    context.strokeStyle = "black";
    context.stroke();

    context.textAlign = "center";
    context.textBaseline = "top";
    ticks.forEach(function(d) {
        context.fillText(tickFormat(d), x(d), size.height + tickSize);
    });
}

function yAxis() {
    var tickCount = 10,
        tickSize = 6,
        tickPadding = 3,
        ticks = y.ticks(tickCount),
        tickFormat = y.tickFormat(tickCount);

    context.beginPath();
    ticks.forEach(function(d) {
        context.moveTo(0, y(d));
        context.lineTo(-6, y(d));
    });
    context.strokeStyle = "black";
    context.stroke();

    context.beginPath();
    context.moveTo(-tickSize, 0);
    context.lineTo(0.5, 0);
    context.lineTo(0.5, size.height);
    context.lineTo(-tickSize, size.height);
    context.strokeStyle = "black";
    context.stroke();

    context.textAlign = "right";
    context.textBaseline = "middle";
    ticks.forEach(function(d) {
        context.fillText(tickFormat(d), -tickSize - tickPadding, y(d));
    });

    context.save();
    context.rotate(-Math.PI / 2);
    context.textAlign = "right";
    context.textBaseline = "top";
    context.font = "bold 10px sans-serif";
    context.fillText("Price (US$)", -10, 10);
    context.restore();
}