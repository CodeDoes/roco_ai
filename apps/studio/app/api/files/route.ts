import { NextRequest, NextResponse } from "next/server";

const ROCO_API = process.env.ROCO_API || "http://localhost:3000";

export async function GET(req: NextRequest) {
  const { searchParams } = new URL(req.url);
  const path = searchParams.get("path");

  try {
    if (path) {
      // Get file content
      const response = await fetch(`${ROCO_API}/files?path=${encodeURIComponent(path)}`);
      const data = await response.json();
      return NextResponse.json(data);
    } else {
      // List files
      const response = await fetch(`${ROCO_API}/files`);
      const data = await response.json();
      return NextResponse.json(data);
    }
  } catch (error) {
    return NextResponse.json(
      { error: "Failed to connect to RoCo API" },
      { status: 500 }
    );
  }
}

export async function PUT(req: NextRequest) {
  try {
    const body = await req.json();
    const response = await fetch(`${ROCO_API}/files`, {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    const data = await response.json();
    return NextResponse.json(data);
  } catch (error) {
    return NextResponse.json(
      { error: "Failed to connect to RoCo API" },
      { status: 500 }
    );
  }
}
